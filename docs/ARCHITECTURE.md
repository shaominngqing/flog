# Architecture

This document describes the high-level architecture of **flog** at the
state it reached after the Phase 3–4 cleanup campaign. It is meant for
contributors and AI agents who need a mental model of the whole system
before touching any one module; file-level detail lives in
[MODULES.md](MODULES.md), wire-format detail in [PROTOCOL.md](PROTOCOL.md),
and contribution rules in [CONTRIBUTING.md](CONTRIBUTING.md).

## 1. What flog is

flog is a terminal-native log viewer and network inspector for Flutter
developers. It runs as an independent long-lived TUI process; any number
of Flutter apps (on any mix of macOS hosts, iOS simulators, iOS real
devices, Android emulators, Android real devices) connect *into* flog
over a WebSocket-based wire protocol and stream their logs + HTTP / SSE /
WebSocket traffic in real time.

flog has two tabs:

- **▤ Logs** — ring-buffered structured log stream with level colours,
  tag pills, regex / pipe-OR search + exclude, tag include / exclude,
  JSON detail panel with a collapsible tree, bookmarks, stats,
  export-to-file, and a Jump-to-Bottom pill that resumes LIVE tail.
- **⇄ Network** — Flipper-style network inspector covering HTTP
  (request / response / headers / body), Server-Sent Events (per-chunk
  list + automatic Merged View for LLM-style streaming APIs), and
  WebSocket (Chat-style grouped view + Raw list). Per-request Replay,
  Copy-as-cURL, Copy-Response, and Mock-rule sync round out the tab.

The design principle that dictates everything else is **terminal-native
first**: every feature must work under ratatui with a 30 Hz render loop,
Catppuccin Macchiato colours, and mouse + keyboard parity. No browser,
no Electron, no GUI.

## 2. Two-process model

```
┌────────────────────────────────┐        WebSocket (port 9753+)        ┌──────────────────────────────┐
│   Flutter app  (flog_dart)     │ ───────────── JSON frames ──────────▶│   flog TUI  (this crate)     │
│                                │                                       │                              │
│  FlogServer (binds 9753..9762) │ ◀──── MockSync / Replay / Subscribe ──│  Transport layer             │
│  FlogDio → Http/Sse/Ws         │                                       │  Input layer (WS client)     │
│  FlogLogger → structured logs  │                                       │  Domain + Parser layers      │
│  FlogStore (50K buffer)        │                                       │  App state machine           │
└────────────────────────────────┘                                       │  UI layer (ratatui)          │
                                                                         └──────────────────────────────┘
```

- **Dart side = data source.** `FlogStore` buffers 50 000 log + network
  frames in-app. Logs happen whether or not flog is attached; when flog
  connects it requests a `Subscribe` and Dart replays its buffer.
- **Rust side = pure renderer + controller.** flog never owns the
  ground-truth data; it renders the frames the app sends, keeps its own
  filtered / derived view state, and mediates the two control messages
  that travel the other direction (mock-rule sync, request replay).

There is no HTTP endpoint, no VM-service dependency, and no persistent
state between apps. Every app restart is a fresh data stream; every
flog restart reconnects to whatever apps are still running.

Both sides agree on a single wire protocol specified in
[PROTOCOL.md](PROTOCOL.md). Versioning is additive: new fields use
`#[serde(default)]` so older clients keep working.

## 3. Four-layer dependency graph

All Rust code lives under `src/`. The dependency arrow runs strictly:

```
              ┌─────────────────┐
              │     ui/         │  ratatui renderers, widgets, theme
              └────────┬────────┘
                       │
              ┌────────▼────────┐
              │     app/        │  App state machine + view states
              │     event/      │  keyboard + mouse dispatch
              │     run/        │  startup wiring + Tokio tasks
              └────────┬────────┘
                       │
         ┌─────────────┴─────────────┐
         │                           │
┌────────▼────────┐         ┌────────▼────────┐
│    domain/      │         │    parser/      │  log-line parser chain
│    session.rs   │         └────────┬────────┘
│    replay.rs    │                  │
└────────┬────────┘         ┌────────▼────────┐
         │                  │     input/      │  WS client + ClientMessage
         │                  └────────┬────────┘
         │                           │
         └──────────┬────────────────┘
                    │
           ┌────────▼────────┐
           │   transport/    │  device discovery + platform connects
           └─────────────────┘
```

**Rules of the graph** (enforced by module structure, not tooling):

1. `domain/` is the trunk. It owns the data types (`LogEntry`,
   `LogLevel`, `NetworkEntry`, `FlogNetKind`, `FilterState`, `NetworkFilter`,
   `MockRule`) and the stores (`LogStore`, `NetworkStore`, `MockRuleStore`).
   It has *zero* UI dependencies — no `ratatui`, no `crossterm`.
2. `parser/` and `input/protocol.rs` depend on `domain/` to produce its
   types. Neither depends on the other.
3. `transport/` is below `input/` in the dep graph but the two layers
   together implement "discover a device → connect a WebSocket → read
   `ClientMessage` frames". `transport/` has zero Tokio-channel traffic
   with the rest of the app; it emits `DeviceEvent`s through one
   `mpsc::UnboundedReceiver` that `run/server.rs` drives.
4. `app/`, `event/`, and `run/` are the controller tier. `app/`
   owns mutable view state (`App`, `LogsViewState`, `NetworkState`,
   `DetailState`, `InputBuffers`, `MockEditState`, `LayoutCache`).
   `event/` maps keyboard + mouse to `App` mutations. `run/` wires the
   Tokio tasks and the render loop; it is the only module that knows
   all of `transport`, `input`, `app`, and `ui`.
5. `ui/` is above everything. Every render function takes `&App` (or a
   sub-state struct) and produces `ratatui::Frame` output. `ui/` never
   writes to `App`.

There are no cycles. `cargo modules dependencies` and manual audit both
confirm this after the Phase 3 redesign; any new dependency that would
introduce one is a red flag.

## 4. Transport layer — device discovery

`src/transport/` turns "what devices does this host have?" into a stream
of `DeviceEvent::Added(Device)` / `DeviceEvent::Removed(String)`
messages on a single `mpsc::UnboundedReceiver`. Three parallel sources
feed that channel:

| Source     | Mechanism                             | Covers                          |
|------------|---------------------------------------|---------------------------------|
| `adb`      | `adb track-devices` stream parser     | Android real devices + AVDs     |
| `usbmuxd`  | `Listen` on macOS `/var/run/usbmuxd`  | iOS real devices over USB       |
| `local`    | TCP probe + WS handshake on localhost | macOS app & iOS simulator       |

Each source lives in its own submodule under
`src/transport/device_monitor/`. A small helper, `DeviceTracker`,
encapsulates the "known set + emit Added / Removed + drain on
disconnect" pattern so that every `Added` event has a matching
`Removed`, even when the underlying stream dies.

### Transport resolution

Given a `Device`, `transport::resolve_transport_addr` (in
`src/transport/resolve.rs`) returns a pure `TransportAddr` value:

```rust
pub enum TransportAddr {
    Localhost { port: u16 },
    AdbForward { serial: String, port: u16 },
    Usbmuxd { device_id: u32, port: u16 },
}
```

This separates "which transport?" (pure, testable) from "run the
platform-specific side effects" (`adb forward`, `usbmuxd Connect`)
which live in `run/server.rs::connection_task`. Audit trail: TRANS-009.

### Port scanning

flog_dart binds the first free port in `[port, port+9]` (10-port range,
matching the scan constant on both sides). flog scans the same range
per discovered device — each (device, port) pair spawns its own
`connection_task`. This lets two Flutter apps run on the same host
with no configuration.

### Reconnect backoff

Each `connection_task` retries forever on disconnect with exponential
backoff: `2s → 4s → 8s → 16s → 30s` (capped). Constants live in
`run/server.rs`:

- `RECONNECT_INITIAL_DELAY_SECS = 2`
- `RECONNECT_MAX_DELAY_SECS = 30`
- `RECONNECT_BACKOFF_FACTOR = 2`

A successful `Connected` event resets the delay. Audit trail:
TRANS-008.

## 5. Input / protocol layer

`src/input/` is the wire-protocol client.

- `input/protocol.rs` — serde types for `ClientMessage` (Hello / Log /
  Net), `ServerMessage` (MockSync / Replay / Subscribe), and
  `ClientInfo`.
- `input/connector.rs` — `connect()` / `connect_stream()` establish a
  WebSocket, perform the Hello handshake with a **3-second timeout**,
  and spawn reader + writer Tokio tasks. Returns
  `(UnboundedReceiver<ConnectorEvent>, ConnectorHandle)`.

The `ConnectorHandle` is the only way for the rest of the system to
send a `ServerMessage` downstream; it hides the `UnboundedSender<String>`
behind typed methods `send_mock_sync`, `send_replay`, `send_subscribe`
(all wrappers around the generic `send(ServerMessage) -> bool`).

### Connection lifecycle

```
connect(ws_url)
  │
  ▼
WS handshake
  │
  ▼
Read first frame (tokio::time::timeout 3s)
  │
  ├── Text + Hello variant ─▶  ConnectorEvent::Connected(ClientInfo)
  ├── Text + other variant ─▶  Err("Expected Hello, got Log/Net/…")
  ├── Binary / close / err ─▶  Err(...)
  └── Timeout              ─▶  Err("Hello handshake timed out after 3s")
  │
  ▼
Reader loop: forward each ClientMessage → ConnectorEvent::Message
Writer loop: ServerMessage JSON from mpsc::UnboundedReceiver
  │
  ▼
Peer close / read error → ConnectorEvent::Disconnected, task exits
```

The 3s timeout is chosen so that legitimate connections (<50 ms on
macOS) succeed trivially while port-scan false matches (a random HTTP
server that accepts the WS upgrade but never sends a Hello) fail fast.

## 6. Domain layer

`src/domain/` is the pure-data trunk. Its public surface (via
`src/domain/mod.rs`):

```
pub use entry::{InputSource, LogEntry, LogLevel, ParseResult};
pub use filter::FilterState;
pub use filter_traits::{FilterVariant, MessageFilter};
pub use network_filter::NetworkFilter;
pub use network_store::NetworkStore;
pub use store::LogStore;
```

### Log storage — `LogStore`

Ring buffer with hard cap `MAX_ENTRIES = 100_000`. On overflow we
`pop_front`, then run a single-pass **fold-on-drain** so adjacent
duplicates (same level + tag + message + extra_lines) merge into one
entry with `repeat_count += 1`. On push we also fold when the new
entry equals the tail — this handles the common log-spam-loop case
cheaply. Audit trail: DOM-011.

### Network storage — `NetworkStore`

Ring buffer with hard cap 10 000. Drives state transitions from
`FlogNetKind` wire frames (`Req` → pending, `Res` → completed, `Chunk`
→ append SSE data, `Send/Recv` → append WS message, etc.). An orphan
`Res` whose id has no matching `Req` is surfaced as
`NetworkEntry::new_orphan_response` rather than silently dropped.
Audit trail: DOM-003.

### Filtering

- `FilterState` — logs filter. Four dimensions applied as one pipeline
  in `FilterState::matches`: min_level → tag include / exclude → search
  (plain OR-terms or `/regex/i`) → exclude search. The `search_regex`
  / `exclude_regex` / `tag_regex` booleans and their compiled `Regex`
  companions are kept in sync by the `set_*` / `parse_tag_filter`
  mutators; external callers cannot desync them.
- `NetworkFilter` — network filter with the same OR-term + regex shape
  plus `ProtocolFilter`, `MethodFilter`, `StatusFilter` enums. The
  three enums share one `FilterVariant` trait (`all()` / `label()` /
  `variants()`) so pill rendering and click handling are
  table-driven. Audit trail: DOM-001.

### Mock rules

`MockRule` has a URL pattern, optional method filter, status code,
response body, optional delay, and enabled flag. `MockRuleStore` owns
the list and serialises it to a JSON string used in the
`ServerMessage::MockSync` frame. Matching semantics are substring,
first-match-wins, case-sensitive on the URL — documented in
flog_dart/lib/src/flog_mock_interceptor.dart. Audit trail: DART-013.

### SSE + WS view helpers

Pure functions only — these don't touch storage or UI state:

- `sse_merge.rs` — `extract_field_paths` scans all chunks for
  leaf-string JSON paths, `resolve_path` walks a parsed value,
  `auto_detect_field` knows OpenAI / Claude streaming patterns,
  `merge_field` concatenates the field across chunks.
- `ws_chat.rs` — `extract_type` scans `type`/`event`/`action`/`op`/
  `cmd`/`method` keys, `has_binary_content` detects base64 >1 KB,
  `group_messages` groups consecutive same-type/direction messages
  and merges delta fields, `preview_message` replaces binary blobs
  with `<binary: N bytes>` labels.

## 7. Parser layer

`src/parser/` turns raw bytes into `LogEntry`s via a chain of
`LogLineParser` strategies:

```
MultiStrategyParser::default_chain()
  = [StructuredParser, GenericParser, KeywordParser]
```

Each strategy returns `Option<LogEntry>`; the first match wins.
`parser::network::try_parse_network` is called separately by
`dispatch_client_message` for entries tagged `flog_net`, because those
carry `FlogNetKind` JSON on the wire rather than a log-looking
string.

- **StructuredParser** — handles `[LEVEL][Tag] msg` and
  `HH:MM:SS.mmm │ LEVEL │ Tag │ msg` pipe format.
- **GenericParser** — Flutter `I/flutter`, `W/1.raster`, VM Service
  timestamps, exception blocks, Dart `DEBUG:` / `ERROR:` prefixes.
- **KeywordParser** — fallback: scans for "error" / "warn" / "debug"
  and tags the line `App` with inferred level. Never returns `None`
  for non-empty input, so the chain never drops a line silently.

Chain construction is behind `MultiStrategyParser::with_strategies`
so tests can A/B alternative chains without touching production code.
Audit trail: DOM-013.

## 8. App layer — state machine

`src/app/` owns `App`: the single mutable root that `main.rs` hands to
`run::run_loop` inside `Arc<Mutex<App>>`. It is split into submodules:

| Submodule       | Responsibility                                                  |
|-----------------|-----------------------------------------------------------------|
| `mod.rs`        | `App` struct, `AppMode`, `ViewTab`, `InputField`                |
| `state_structs` | `SearchState`, `InputBuffers`, `DetailState`, `LogsViewState`, `StatsSnapshot` |
| `network_state` | `NetworkState` — all Network-tab view state                     |
| `multi_app`     | `ConnectedApp`, add/remove/switch invariants                    |
| `mode`          | Mode transition helpers (enter / exit Input / Help / Stats)     |
| `scroll`        | Logs-tab scroll primitives (`move_up/down`, `select_up/down`)   |
| `layout_cache`  | `LayoutCache` — per-frame rect + click-region snapshots         |
| `mock_edit`     | `MockEditState` — the mock-rule editor bundle                   |
| `sse_merge`     | `SseMergeRule`, `SsePathSegment` — SSE Merged View rule types   |
| `detail`        | Detail-panel helpers shared by both tabs                        |
| `input_fields`  | Shared input-field dispatch across the 5 unified fields         |

### `AppMode`

```rust
pub enum AppMode {
    Normal,
    InputActive(InputField),  // one of 5 fields (Logs tab: Search/Exclude/Tag; Network: Search/Exclude)
    Help,
    Stats,
    MockRuleEdit,
    FullValueOverlay(FullValueOverlayState),  // expanded string viewer (JSON detail)
}
```

`InputField` carries a `tab()` method so dispatchers can assert the
active field belongs to the active tab. Audit trail: UI-002.

### `LogsViewState` and `NetworkState` symmetry

Both structs hold `selected`, `scroll_offset`, `auto_scroll`,
a write-through `filtered_indices` cache, plus tab-specific
extensions (detail state, mock panel flags, SSE / WS mode toggles).
The symmetry was introduced by Phase 3 Step 3.10 (audit UI-003) and
completed in Phase 4 when the Logs-side delegate fields on `App` were
removed — `app.logs` is now the single source of truth for the Logs
tab's viewport.

### Multi-app invariants

`connected_apps`, `active_app_id`, and `discovered_devices` cooperate
to track "which apps are attached, and which one is being viewed". The
invariants (documented on `App` itself) are load-bearing for the
switch-app picker flow; audit trail: UI-040 + UI-023.

## 9. Event layer — two-phase mouse dispatch

`src/event/` turns keyboard + mouse events into `App` mutations.

Top-level routing fans out by `AppMode`:

```
AppMode::Normal         → handle_normal_key / handle_normal_mouse
AppMode::InputActive(f) → handle_input_key(f, …) / handle_input_mouse
AppMode::Help | Stats   → handle_overlay_key / handle_overlay_mouse
AppMode::MockRuleEdit   → handle_mock_edit_key / handle_mock_edit_mouse
```

Each sub-handler ends with a deliberate no-op catch-all; unhandled
keys / mouse events are swallowed silently by design.

### Two-phase mouse: detect → apply

Phase 3 Step 3.6 split mouse handling into:

1. **`detect::detect_click_region(&App, x, y) -> Option<ClickRegion>`** —
   pure, read-only, exhaustively unit-testable.
2. **`apply::apply_click_region(&mut App, region, class, x, y)`** —
   performs the mutation.

`ClickRegion` (in `event/click_region.rs`) is the semantic enum.
Representative variants:

- Tab bar: `LogsTab`, `NetworkTab`
- Logs toolbar: `LogsToolbarLevel(LogLevel)`, `LogsToolbarSearch`,
  `LogsToolbarTag`, `LogsToolbarExclude`, `LogsListRow { row }`,
  `LogsJumpToBottom`, `LogsDetailPanel { line_idx, x }`
- Network toolbar: `NetworkToolbarSearch`, `NetworkProtocolPill(...)`,
  `NetworkMethodPill(...)`, `NetworkStatusPill(...)`, `NetworkListRow`,
  `NetworkDetailPanel`, `NetworkDetailSseEventsPill`,
  `NetworkDetailSseMergedPill`, `NetworkDetailSseFieldPill`,
  `NetworkDetailWsChatPill`, `NetworkDetailMockBtn`, `NetworkDetailReplayBtn`
- Mock rules panel: `MockRuleRow`, `MockRuleEditBtn`, `MockRuleToggle`,
  `MockRuleDelete`, `MockRuleAdd`, `MockRuleClose`
- Status bar + scrollbar: `StatusBar`, `Scrollbar { axis, direction }`

This split makes it possible to characterization-test "did clicking at
(x, y) land on the Network tab label?" without also needing to reason
about every downstream mutation. Audit trail: UI-009 + UI-041.

## 10. Run layer — startup + Tokio tasks

`src/run/` is small (three files); it was extracted from `main.rs` in
Phase 4 so `main()` itself is now 93 lines of CLI parse + terminal
lifecycle + panic hook. The extracted pieces:

- `run/server.rs` — spawns the device-discovery → per-connection
  fanout. Owns the `active_tasks` + `adb_forwards` maps so every
  adb-forward rule gets torn down on device removal.
- `run/dispatch.rs` — `dispatch_client_message(&mut App, ClientMessage)`,
  the single place where `Log` → `LogStore` and `Net` → `NetworkStore`
  conversions happen. Also exposes `format_ts`, `split_stacktrace`, and
  the raw-log `RAW_LOG_RE` regex for test reuse.
- `run/render_loop.rs` — 30 Hz render loop, polls crossterm events,
  calls `event::handle_key` / `handle_mouse`, invokes
  `ui::draw(&mut frame, &mut app)`.

## 11. UI layer

`src/ui/` is ratatui-driven. Every renderer takes `&App` (or a
projection) and produces widgets. The layer is organised as:

- `mod.rs` — top-level dispatcher, shared palette constants
  (Catppuccin Macchiato), theme helpers.
- `tab_bar.rs` — ▤ Logs / ⇄ Network tab bar renderer.
- `logs/` — Logs view: toolbar, list, status bar, detail panel,
  stats overlay, jump-to-bottom pill, empty states, highlight.
- `network/` — Network view: toolbar with filter pills, request table,
  detail panel (General, Query Params, Headers, Body, SSE / WS
  sections), stats overlay, mock-rules side panel.
- `json_viewer/` — **Shared** collapsible JSON tree component. AST-based
  (`serde_json::Value` + DFS flatten), parallel `Vec<bool>` fold
  state keyed by node id, depth-aware rendering with DevTools-style
  collapsed summaries (`{k: v, …}` / `[v, …] (N)`), fixed-width ▼/▶
  markers, CJK-aware truncation. Used by both Logs detail and Network
  detail. Interactive features added in 2026-05:
  - `action.rs` — `JsonAction` enum (`ToggleFold` / `CopyNode` / `OpenUrl` /
    `ExpandFullValue`) and `JsonHotRegion` (column range + action); the
    render phase populates a per-frame click map, the detect/apply
    two-phase pattern is used for JSON actions exactly as it is for
    all other mouse clicks (audit UI-009).
  - `viewer_cursor` on `DetailState` — row-level keyboard cursor for
    the JSON viewer; `J`/`K` navigate rows, `Enter` activates the best
    action, `o` opens a URL, `y` copies the node.
  - `AppMode::FullValueOverlay(FullValueOverlayState)` — full-screen
    overlay that displays a truncated string value in a 70 × 70 %
    centred modal with scroll, Enter/click to copy, Esc to close.
    Renderer lives in `ui/full_value_overlay.rs`.
- `input_field/` — **Shared** single-line input widget with cursor +
  viewport scroll. Used by the 5 unified input fields.
- `text_editor/` — **Shared** multi-line text editor. Used by the
  mock-rule body editor.
- `device_picker/` — device + app picker overlay (opened from the
  status bar).
- `help/` — help overlay with per-tab content sections under
  `help/content/logs.rs` + `help/content/network.rs`.

## 12. Concurrency model

flog runs on `tokio` with the multi-threaded flavour. Background tasks:

1. **Device discovery** (3 tasks) — one per source (`adb`, `usbmuxd`,
   `local`). Each pushes `DeviceEvent`s into a single unbounded channel.
2. **Device fanout** (`run::spawn_device_discovery`) — consumes
   `DeviceEvent`s, spawns / aborts `connection_task`s per
   (device, port) pair.
3. **Connection tasks** — one per (device, port). Each runs the retry
   loop; on success it spawns the two connector tasks below.
4. **Connector reader task** — reads WS frames, decodes
   `ClientMessage`, forwards as `ConnectorEvent`.
5. **Connector writer task** — pulls `String`s from the
   `ConnectorHandle` mpsc channel, writes them as WS text frames.
6. **Switch-app handler** (`run::spawn_switch_app_handler`) — consumes
   UI "switch to this app" requests, mutates `active_app_id`, sends a
   `Subscribe` downstream.
7. **Render loop** (`run::run_loop`) — main-thread 30 Hz poll;
   processes crossterm events and draws the frame.

Shared state is exactly one `Arc<Mutex<App>>`. No other synchronised
state. Each Tokio task that touches `App` does so briefly (wrap
mutations in the smallest possible critical section).

## 13. Scroll model

Both Logs and Network use the same shape:

- `move_up/down(n)` — viewport scroll (mouse wheel, PgUp/PgDn).
  Disables `auto_scroll` and moves both `scroll_offset` and
  `selected`.
- `select_up/down(n)` — cursor move (j/k). Disables `auto_scroll`,
  moves only `selected`; viewport follows if needed.
- `go_top/go_bottom` — Home / End.

**The renderer is the scroll authority.** `scroll_offset` is clamped
inside the draw function once per frame; bottom-detection (for
`auto_scroll` resume) is likewise a render-time decision. This lets
mutation code stay ignorant of viewport height.

**Logcat-strict never-disturb.** Once the user scrolls off the tail
(`auto_scroll = false`), incoming logs never move their cursor. The
Jump-to-Bottom pill + `new_logs_since_pause` counter are how the user
sees that there's buffered content waiting.

Both `LogsViewState` and `NetworkState` own their own `auto_scroll`
field. The flag is never mirrored on `App`. Audit trail: UI-006.

## 14. Mock + Replay

### Mock

```
User creates rule in flog TUI
  │
  ▼
MockRuleStore updated
  │
  ▼
MockRuleStore::to_json_string()
  │
  ▼
ConnectorHandle::send_mock_sync(rules_json)
  │  (per connected app)
  ▼
ServerMessage::MockSync { rules }  ───WS───▶  flog_dart
  │                                            │
  │                                            ▼
  │                                      FlogMockInterceptor.updateRules()
  │                                            │
  │                                            ▼
  │                                      Next matching Dio request
  │                                      resolves locally with the
  │                                      canned response; marked
  │                                      mocked=true on the Res frame.
  ▼
Res arrives back with mocked=true → NetworkEntry.source = Mocked
(UI shows "Mocked" badge + warm highlight row)
```

Mock matching (substring, first-match-wins, case-sensitive on URL) is
performed in the Dart interceptor, not in flog — flog is just the
rule authoring tool.

### Replay

```
User hits Replay from Network detail
  │
  ▼
ConnectorHandle::send_replay(method, url, headers, body)
  │
  ▼
ServerMessage::Replay { method, url, headers, body }  ──WS──▶  flog_dart
  │                                                             │
  │                                                             ▼
  │                                                       Dio re-executes
  │                                                       the request (may
  │                                                       or may not hit a
  │                                                       mock rule).
  ▼
New NetworkEntry arrives with source=Replay (distinct from App/Mocked)
```

## 15. Error handling philosophy

- **Never drop log lines.** The parser chain ends in `KeywordParser`
  which accepts anything non-empty. Unrecognised WS messages are
  dropped at the `ClientMessage` deserialize boundary only (older
  clients sending unknown variants), and the `ClientMessage` match is
  exhaustive so adding a variant is a compile-time change.
- **Orphan responses are surfaced, not silenced.** `NetworkStore`
  creates a synthetic orphan entry for `Res` frames with no matching
  `Req`. Audit trail: DOM-003.
- **Reader / writer task exits are logged to stderr.** Quiet exits
  would make stale-connection symptoms invisible; we accept the
  terminal noise trade-off. Audit trail: TRANS-006.
- **Port-scan misfires fail fast.** The 3-second Hello timeout bounds
  how long a connection to a non-flog HTTP server can tie up a port
  slot. Audit trail: TRANS-005.
- **Retry is silent per attempt.** The reconnect loop does not emit
  status-bar toasts per 2 s–30 s cycle; the user sees the
  reader/writer stderr lines instead. Audit trail: TRANS-011.

## 16. Cross-references

- Per-module detail: [MODULES.md](MODULES.md).
- Wire protocol: [PROTOCOL.md](PROTOCOL.md).
- Contribution rules (audit taxonomy, testing conventions, file size
  budget, commit format): [CONTRIBUTING.md](CONTRIBUTING.md).
- Audit trail (raw findings behind every "Audit trail: X-nnn" reference
  in this doc): [docs/superpowers/audit/](superpowers/audit/).
- Campaign journals (Phase 0 through Phase 5): [docs/superpowers/journal/](superpowers/journal/).
- flog_dart companion package: [flog_dart/README.md](../flog_dart/README.md)
  + [flog_dart/CHANGELOG.md](../flog_dart/CHANGELOG.md).
