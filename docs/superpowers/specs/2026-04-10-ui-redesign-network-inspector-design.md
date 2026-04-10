# flog UI Redesign + Network Inspector

Date: 2026-04-10

## Overview

Two major improvements to flog:

1. **Logs view visual overhaul** — better readability through spacing, color hierarchy, and layout restructuring
2. **Network Inspector view** — Flipper-style network request viewer with structured data from Dart interceptors, supporting HTTP/SSE/WebSocket

## Architecture

### Dual-View Tab System

Top-level `ViewTab` enum: `Logs` | `Network`. Each view is a fully independent module with its own toolbar, content area, state, and event handling.

Layout:
```
┌──────────────────────────────────────────────────────┐
│   Logs       Network                                 │  Tab bar (global, MANTLE bg)
├──────────────────────────────────────────────────────┤
│   [view-specific toolbar]                            │
├──────────────────────────────────────────────────────┤
│                                                      │
│   [view-specific content + optional detail panel]    │
│                                                      │
├──────────────────────────────────────────────────────┤
│   [view-specific status bar]                         │
└──────────────────────────────────────────────────────┘
```

Tab bar: active tab = BLUE text + underline, inactive = OVERLAY0. Keyboard: `1`=Logs, `2`=Network. Mouse: click to switch.

### Code Structure

```
src/ui/
├── mod.rs              Top-level draw: tab bar + dispatch to view
├── tab_bar.rs          Tab bar renderer
├── logs/               Logs view (refactored from current code)
│   ├── mod.rs          Logs main render (toolbar + list + timeline + status)
│   ├── detail.rs       Log detail panel
│   ├── highlight.rs    Auto-highlight rules
│   ├── timeline.rs     Timeline heatmap
│   └── stats.rs        Statistics view
├── network/            Network view (new)
│   ├── mod.rs          Network main render (toolbar + table + status)
│   ├── detail.rs       Request detail panel (sections: General, Headers, Body, Stream)
│   └── filter.rs       Method/Status/Protocol dropdown filters
├── help.rs
└── source_select.rs

src/domain/
├── entry.rs            LogEntry (existing)
├── network.rs          NetworkEntry, SseChunk, WsMessage (new)
├── filter.rs           LogFilter (existing)
├── network_filter.rs   NetworkFilter (new)
├── store.rs            LogStore (existing)
└── network_store.rs    NetworkStore (new)

src/parser/
├── ...                 Existing parsers
└── network.rs          Parse [flog_net] tagged lines → NetworkEntry

src/app.rs              ViewTab enum, LogsState, NetworkState
```

### App State

```rust
enum ViewTab { Logs, Network }

struct App {
    active_tab: ViewTab,
    logs: LogsState,       // all current log-view fields moved here
    network: NetworkState,  // new
    // shared: store, network_store, connected, source_name, ...
}
```

---

## Part 1: Logs View Visual Overhaul

### 1.1 Row Separator

Add `Modifier::UNDERLINED` to the last line of every log entry. Color: `SURFACE0`(#363a4f). Pad lines with spaces to full width so underline extends edge-to-edge. Zero extra vertical space.

### 1.2 Error/Warning Row Background

```rust
const ERROR_ROW_BG: Color = Color::Rgb(50, 30, 35);
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);
```

Replace zebra striping. Non-error/warning rows use uniform `BASE` background.

### 1.3 Message Color by Level

| Level   | Message Color         | Notes              |
|---------|-----------------------|--------------------|
| Error   | RED (#ed8796)         | Most prominent     |
| Warning | YELLOW (#eed49f)      | Prominent          |
| Info    | TEXT (#cad3f5)        | Normal             |
| Debug   | SUBTEXT0 (#a5adce)    | Slightly dimmed    |
| Verbose | OVERLAY0 (#6e738d)    | Clearly dimmed     |
| System  | OVERLAY0 (#6e738d)    | Clearly dimmed     |

### 1.4 Column Reorder

New order: `cursor(1) | bookmark(2) | level_pill(9) | sep(1) | timestamp(12) | sep(1) | tag_pill(14) | sep(1) | message(flex)`

Level moves before timestamp — it is the primary scan anchor.

### 1.5 Tag Colored Pills

Tags get colored pill backgrounds (MANTLE text on color bg). 5-color palette: BLUE, GREEN, PEACH, MAUVE, SAPPHIRE. Color assigned by tag name hash, consistent within session.

### 1.6 Selected Row

- Left `▎` indicator: BLUE (unchanged)
- Background: SURFACE1 (#494d64), more visible than current SURFACE0
- Underline on selected row also uses SURFACE1

### 1.7 HTTP Highlight Enhancement

In `highlight.rs`:
- HTTP methods (GET/POST/...) — pill style (dark text on MAUVE bg)
- HTTP status codes — keep colors, add BOLD
- Duration >1000ms — RED + BOLD + UNDERLINED

---

## Part 2: Network Inspector View

### 2.1 NetworkEntry

```rust
enum Protocol { Http, Sse, Ws }
enum NetworkStatus { Pending, Active, Completed, Failed }

struct NetworkEntry {
    id: u64,
    protocol: Protocol,
    timestamp: String,
    method: String,                     // GET/POST/... or empty for WS
    url: String,
    path: String,                       // extracted for display
    status: NetworkStatus,
    http_status: Option<u16>,
    duration: Option<u64>,              // ms
    request_size: Option<u64>,
    response_size: Option<u64>,
    request_headers: Option<String>,
    response_headers: Option<String>,
    request_body: Option<String>,
    response_body: Option<String>,
    error: Option<String>,
    // SSE-specific
    sse_chunks: Vec<SseChunk>,
    sse_total_size: u64,
    // WS-specific
    ws_messages: Vec<WsMessage>,
    ws_close_code: Option<u16>,
    ws_close_reason: Option<String>,
}

struct SseChunk { seq: u32, data: String, size: u64 }

enum WsDirection { Send, Recv }
struct WsMessage { direction: WsDirection, data: String, size: u64, timestamp: String }
```

### 2.2 Network List Columns

```
PROTO   METHOD   URL                     STATUS    TIME     SIZE
─────────────────────────────────────────────────────────────────
 HTTP    GET     /api/episodes             200    120ms    1.2K
 HTTP    POST    /api/user-goals           200    340ms    0.3K
 SSE     POST    /v1/chat/completions    ●stream   3.2s    8.0K
 WS      ──      wss://gemini.google..   ●live    45.0s     12K
 HTTP    DELETE  /api/cache                500     1.2s    0.1K
```

Visual rules:
- PROTO column: HTTP=SUBTEXT0, SSE=PEACH pill, WS=SAPPHIRE pill
- METHOD column: pill style per method (GET=GREEN, POST=BLUE, PUT=PEACH, DELETE=RED, PATCH=MAUVE)
- STATUS: 2xx=GREEN, 3xx=BLUE, 4xx=YELLOW, 5xx=RED+BOLD
- Duration: <200ms normal, 200-1000ms PEACH, >1000ms RED+BOLD
- Active SSE/WS: pulsing dot ● in status column
- Row background: 4xx=WARNING_ROW_BG, 5xx/error=ERROR_ROW_BG
- Row separator: UNDERLINED with SURFACE0 (same as Logs)

### 2.3 Network Toolbar

```
/filter url...    Protocol ▼    Method ▼    Status ▼
```

- URL text filter (regex support)
- Protocol dropdown: All / HTTP / SSE / WS
- Method dropdown: All / GET / POST / PUT / DELETE / PATCH
- Status dropdown: All / 2xx / 3xx / 4xx / 5xx / Failed
- All filters are mouse-clickable dropdowns

### 2.4 Network Detail Panel

Right-side panel (same split as Logs detail). Sections are collapsible (click or keyboard).

**HTTP detail:**
- General: URL, Method, Status, Duration, Size
- Request Headers (key-value, MAUVE keys)
- Request Body (JSON highlighted)
- Response Headers
- Response Body (JSON highlighted)

**SSE detail:**
- General: URL, Method, Status, Duration, Total Chunks, Total Size
- Request Headers
- Request Body
- Stream Events: numbered chunks with JSON highlight, scrollable, shows recent 10 by default

**WS detail:**
- General: URL, Status, Duration, Messages (N sent / M received)
- Messages: chronological list, → send in GREEN, ← recv in BLUE, binary shown as `<binary: N bytes>`

### 2.5 Network Mouse Operations

| Action | Effect |
|--------|--------|
| Click row | Select, open detail panel |
| Double-click row | Select + full-screen detail |
| Click column header | Sort by column (toggle asc/desc) |
| Click Protocol/Method/Status ▼ | Open filter dropdown |
| Scroll wheel | Scroll list |
| Click detail section header | Collapse/expand |
| Right-click row | Copy URL to clipboard |

---

## Part 3: Dart flog_logger Extension

### 3.1 Protocol Format

Tag: `[INFO][flog_net]`. JSON payload with `id`, `t` (type), `p` (protocol).

**HTTP:**
```json
{"id":1,"t":"req","p":"http","method":"GET","url":"...","headers":{...},"body":null}
{"id":1,"t":"res","p":"http","status":200,"duration":120,"headers":{...},"body":"...","size":1234}
{"id":1,"t":"err","p":"http","error":"Connection timeout","duration":5000}
```

**SSE:**
```json
{"id":2,"t":"req","p":"sse","method":"POST","url":"...","headers":{...},"body":"..."}
{"id":2,"t":"chunk","p":"sse","data":"...","seq":1}
{"id":2,"t":"done","p":"sse","duration":3200,"chunks":42,"size":8192}
{"id":2,"t":"err","p":"sse","error":"Stream interrupted","duration":1500}
```

**WebSocket:**
```json
{"id":3,"t":"open","p":"ws","url":"wss://..."}
{"id":3,"t":"send","p":"ws","data":"...","size":256}
{"id":3,"t":"recv","p":"ws","data":"...","size":1024}
{"id":3,"t":"close","p":"ws","code":1000,"reason":"Normal closure","duration":45000}
{"id":3,"t":"err","p":"ws","error":"Connection failed"}
```

### 3.2 Dart Classes

```dart
// HTTP interceptor for dio
class FlogHttpInterceptor extends Interceptor {
  FlogHttpInterceptor({
    this.includeRequestHeaders = true,
    this.includeResponseHeaders = true,
    this.includeRequestBody = true,
    this.includeResponseBody = true,
    this.maxBodySize = 10 * 1024,
    this.filter,
  });
}

// SSE wrapper
class FlogSseParser {
  static Stream<SseEvent> parse(
    Stream<List<int>> byteStream, {
    required String url,
    required String method,
    Map<String, dynamic>? headers,
    String? requestBody,
  }) async* { ... }
}

// WebSocket wrapper
class FlogWebSocket {
  static Future<FlogWebSocket> connect(
    String url, {
    Map<String, dynamic>? headers,
  }) async { ... }

  void send(dynamic data) { ... }
  Stream<dynamic> get stream => ...;
  Future<void> close([int? code, String? reason]) async { ... }
}
```

### 3.3 Configuration

```dart
FlogHttpInterceptor(
  includeRequestHeaders: true,
  includeResponseHeaders: true,
  includeRequestBody: true,
  includeResponseBody: true,
  maxBodySize: 10 * 1024,        // truncate body > 10KB
  filter: (request) => true,      // optional: skip certain requests
)
```

### 3.4 Backward Compatibility

- Existing FlogLogger API unchanged
- Network view shows guidance when empty: "Add FlogHttpInterceptor to see network requests"
- flog_logger remains pure Dart, no Flutter SDK dependency
- flog Rust side: unrecognized `flog_net` JSON silently dropped (forward compatible)
