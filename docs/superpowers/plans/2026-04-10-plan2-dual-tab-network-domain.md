# Plan 2: Dual-Tab Architecture + Network Domain

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce the dual-tab system (Logs/Network), extract Logs state into `LogsState`, add `NetworkEntry`/`NetworkStore`/`NetworkFilter` domain types, add `flog_net` protocol parser, and wire the data pipeline so network events flow from source → parser → store.

**Architecture:** Structural refactor: split App into ViewTab + per-view state, move UI files into `src/ui/logs/` subdirectory, add `src/domain/network.rs` and `src/domain/network_store.rs`, add `src/parser/network.rs`. The Logs view UI continues to work identically after the refactor. The Network view starts as an empty placeholder.

**Tech Stack:** Rust, ratatui 0.29, serde_json (already in deps)

**Depends on:** Plan 1 (Logs Visual Overhaul) must be completed first.

---

### Task 1: Add Network Domain Types

**Files:**
- Create: `src/domain/network.rs`
- Modify: `src/domain/mod.rs`

- [ ] **Step 1: Create `src/domain/network.rs`**

```rust
//! Network request data types for HTTP, SSE, and WebSocket.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Sse,
    Ws,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Sse => "SSE",
            Self::Ws => "WS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkStatus {
    Pending,
    Active,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsDirection {
    Send,
    Recv,
}

#[derive(Debug, Clone)]
pub struct SseChunk {
    pub seq: u32,
    pub data: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct WsMessage {
    pub direction: WsDirection,
    pub data: String,
    pub size: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone)]
pub struct NetworkEntry {
    pub id: u64,
    pub protocol: Protocol,
    pub timestamp: String,
    pub method: String,
    pub url: String,
    pub path: String,
    pub status: NetworkStatus,
    pub http_status: Option<u16>,
    pub duration: Option<u64>,
    pub request_size: Option<u64>,
    pub response_size: Option<u64>,
    pub request_headers: Option<String>,
    pub response_headers: Option<String>,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
    pub error: Option<String>,
    // SSE-specific
    pub sse_chunks: Vec<SseChunk>,
    pub sse_total_size: u64,
    // WS-specific
    pub ws_messages: Vec<WsMessage>,
    pub ws_close_code: Option<u16>,
    pub ws_close_reason: Option<String>,
}

impl NetworkEntry {
    pub fn new_http(id: u64, method: String, url: String, timestamp: String) -> Self {
        let path = extract_path(&url);
        Self {
            id,
            protocol: Protocol::Http,
            timestamp,
            method,
            url,
            path,
            status: NetworkStatus::Pending,
            http_status: None,
            duration: None,
            request_size: None,
            response_size: None,
            request_headers: None,
            response_headers: None,
            request_body: None,
            response_body: None,
            error: None,
            sse_chunks: Vec::new(),
            sse_total_size: 0,
            ws_messages: Vec::new(),
            ws_close_code: None,
            ws_close_reason: None,
        }
    }

    pub fn new_sse(id: u64, method: String, url: String, timestamp: String) -> Self {
        let path = extract_path(&url);
        Self {
            id,
            protocol: Protocol::Sse,
            timestamp,
            method,
            url,
            path,
            status: NetworkStatus::Active,
            http_status: None,
            duration: None,
            request_size: None,
            response_size: None,
            request_headers: None,
            response_headers: None,
            request_body: None,
            response_body: None,
            error: None,
            sse_chunks: Vec::new(),
            sse_total_size: 0,
            ws_messages: Vec::new(),
            ws_close_code: None,
            ws_close_reason: None,
        }
    }

    pub fn new_ws(id: u64, url: String, timestamp: String) -> Self {
        let path = extract_path(&url);
        Self {
            id,
            protocol: Protocol::Ws,
            timestamp,
            method: String::new(),
            url,
            path,
            status: NetworkStatus::Active,
            http_status: None,
            duration: None,
            request_size: None,
            response_size: None,
            request_headers: None,
            response_headers: None,
            request_body: None,
            response_body: None,
            error: None,
            sse_chunks: Vec::new(),
            sse_total_size: 0,
            ws_messages: Vec::new(),
            ws_close_code: None,
            ws_close_reason: None,
        }
    }

    /// Total size for display (response_size for HTTP, sse_total_size for SSE,
    /// sum of ws_messages sizes for WS).
    pub fn display_size(&self) -> u64 {
        match self.protocol {
            Protocol::Http => self.response_size.unwrap_or(0),
            Protocol::Sse => self.sse_total_size,
            Protocol::Ws => self.ws_messages.iter().map(|m| m.size).sum(),
        }
    }
}

/// Extract path from URL for compact display.
fn extract_path(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let after_scheme = &url[pos + 3..];
        if let Some(slash) = after_scheme.find('/') {
            return after_scheme[slash..].to_string();
        }
    }
    url.to_string()
}

/// JSON message types from flog_net protocol.
#[derive(Debug, Deserialize)]
pub struct FlogNetMessage {
    pub id: u64,
    pub t: String,          // req, res, err, chunk, done, open, send, recv, close
    pub p: Option<String>,  // http, sse, ws
    pub method: Option<String>,
    pub url: Option<String>,
    pub status: Option<u16>,
    pub duration: Option<u64>,
    pub headers: Option<serde_json::Value>,
    pub body: Option<String>,
    pub size: Option<u64>,
    pub data: Option<String>,
    pub seq: Option<u32>,
    pub chunks: Option<u32>,
    pub code: Option<u16>,
    pub reason: Option<String>,
    pub error: Option<String>,
}
```

- [ ] **Step 2: Update `src/domain/mod.rs` to export new types**

```rust
pub mod entry;
pub mod filter;
pub mod network;
pub mod store;

pub use entry::{InputSource, LogEntry, LogLevel, ParseResult};
pub use filter::FilterState;
pub use network::{
    FlogNetMessage, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
};
pub use store::LogStore;
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles (new types are defined but not yet used, which is fine — Rust allows unused code with warnings).

- [ ] **Step 4: Commit**

```bash
git add src/domain/network.rs src/domain/mod.rs
git commit -m "feat(domain): add NetworkEntry, Protocol, SseChunk, WsMessage types"
```

---

### Task 2: Add NetworkStore

**Files:**
- Create: `src/domain/network_store.rs`
- Modify: `src/domain/mod.rs`

- [ ] **Step 1: Create `src/domain/network_store.rs`**

```rust
//! Storage for network entries with id-based lookup.

use crate::domain::network::{
    FlogNetMessage, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
};

const MAX_ENTRIES: usize = 10_000;
const DRAIN_COUNT: usize = 1_000;

pub struct NetworkStore {
    entries: Vec<NetworkEntry>,
}

impl NetworkStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(256),
        }
    }

    /// Process a parsed flog_net message, creating or updating entries.
    /// Returns the number of drained entries (0 in most cases).
    pub fn process_message(&mut self, msg: FlogNetMessage) -> usize {
        let protocol = match msg.p.as_deref() {
            Some("sse") => Protocol::Sse,
            Some("ws") => Protocol::Ws,
            _ => Protocol::Http,
        };

        match msg.t.as_str() {
            "req" => self.handle_request(msg, protocol),
            "res" => { self.handle_response(msg); 0 }
            "err" => { self.handle_error(msg); 0 }
            "chunk" => { self.handle_sse_chunk(msg); 0 }
            "done" => { self.handle_sse_done(msg); 0 }
            "open" => self.handle_ws_open(msg),
            "send" => { self.handle_ws_send(msg); 0 }
            "recv" => { self.handle_ws_recv(msg); 0 }
            "close" => { self.handle_ws_close(msg); 0 }
            _ => 0,
        }
    }

    fn handle_request(&mut self, msg: FlogNetMessage, protocol: Protocol) -> usize {
        let timestamp = String::new(); // filled by caller if needed
        let method = msg.method.unwrap_or_default();
        let url = msg.url.unwrap_or_default();

        let mut entry = match protocol {
            Protocol::Http => NetworkEntry::new_http(msg.id, method, url, timestamp),
            Protocol::Sse => NetworkEntry::new_sse(msg.id, method, url, timestamp),
            Protocol::Ws => NetworkEntry::new_http(msg.id, method, url, timestamp), // shouldn't happen
        };
        entry.request_headers = msg.headers.map(|v| v.to_string());
        entry.request_body = msg.body;
        if let Some(size) = msg.size {
            entry.request_size = Some(size);
        }

        self.entries.push(entry);
        self.drain_if_full()
    }

    fn handle_response(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.http_status = msg.status;
            entry.duration = msg.duration;
            entry.response_headers = msg.headers.map(|v| v.to_string());
            entry.response_body = msg.body;
            entry.response_size = msg.size;
            entry.status = NetworkStatus::Completed;
        }
    }

    fn handle_error(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.error = msg.error;
            entry.duration = msg.duration;
            entry.status = NetworkStatus::Failed;
        }
    }

    fn handle_sse_chunk(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            let data = msg.data.unwrap_or_default();
            let size = msg.size.unwrap_or(data.len() as u64);
            entry.sse_chunks.push(SseChunk {
                seq: msg.seq.unwrap_or(entry.sse_chunks.len() as u32 + 1),
                data,
                size,
            });
            entry.sse_total_size += size;
        }
    }

    fn handle_sse_done(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.duration = msg.duration;
            if let Some(size) = msg.size {
                entry.sse_total_size = size;
            }
            entry.status = NetworkStatus::Completed;
        }
    }

    fn handle_ws_open(&mut self, msg: FlogNetMessage) -> usize {
        let url = msg.url.unwrap_or_default();
        let entry = NetworkEntry::new_ws(msg.id, url, String::new());
        self.entries.push(entry);
        self.drain_if_full()
    }

    fn handle_ws_send(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            let data = msg.data.unwrap_or_default();
            let size = msg.size.unwrap_or(data.len() as u64);
            entry.ws_messages.push(WsMessage {
                direction: WsDirection::Send,
                data,
                size,
                timestamp: String::new(),
            });
        }
    }

    fn handle_ws_recv(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            let data = msg.data.unwrap_or_default();
            let size = msg.size.unwrap_or(data.len() as u64);
            entry.ws_messages.push(WsMessage {
                direction: WsDirection::Recv,
                data,
                size,
                timestamp: String::new(),
            });
        }
    }

    fn handle_ws_close(&mut self, msg: FlogNetMessage) {
        if let Some(entry) = self.find_by_id_mut(msg.id) {
            entry.ws_close_code = msg.code;
            entry.ws_close_reason = msg.reason;
            entry.duration = msg.duration;
            entry.status = NetworkStatus::Completed;
        }
    }

    fn find_by_id_mut(&mut self, id: u64) -> Option<&mut NetworkEntry> {
        self.entries.iter_mut().rev().find(|e| e.id == id)
    }

    fn drain_if_full(&mut self) -> usize {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.drain(..DRAIN_COUNT);
            DRAIN_COUNT
        } else {
            0
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&NetworkEntry> {
        self.entries.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &NetworkEntry> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
```

- [ ] **Step 2: Update `src/domain/mod.rs`**

Add the new module and re-export:

```rust
pub mod entry;
pub mod filter;
pub mod network;
pub mod network_store;
pub mod store;

pub use entry::{InputSource, LogEntry, LogLevel, ParseResult};
pub use filter::FilterState;
pub use network::{
    FlogNetMessage, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
};
pub use network_store::NetworkStore;
pub use store::LogStore;
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/domain/network_store.rs src/domain/mod.rs
git commit -m "feat(domain): add NetworkStore with flog_net message processing"
```

---

### Task 3: Add Network Protocol Parser

**Files:**
- Create: `src/parser/network.rs`
- Modify: `src/parser/mod.rs`

- [ ] **Step 1: Create `src/parser/network.rs`**

```rust
//! Parser for flog_net protocol messages.
//!
//! Recognizes log lines with tag `flog_net` and parses the JSON payload
//! into FlogNetMessage for routing to NetworkStore.

use crate::domain::network::FlogNetMessage;

const FLOG_NET_TAG: &str = "flog_net";

/// Try to parse a LogEntry as a flog_net network message.
/// Returns Some(FlogNetMessage) if the entry's tag is "flog_net" and the message is valid JSON.
pub fn try_parse_network(tag: &str, message: &str) -> Option<FlogNetMessage> {
    if tag != FLOG_NET_TAG {
        return None;
    }
    serde_json::from_str(message).ok()
}
```

- [ ] **Step 2: Update `src/parser/mod.rs` to export the network parser**

Add the module declaration:

```rust
pub mod structured;
pub mod generic;
pub mod keyword;
pub mod network;
```

No changes to `MultiStrategyParser` — the network parser is called separately in the data pipeline (in `app.rs`), not as part of the strategy chain. Regular log lines tagged `flog_net` will be parsed by the structured parser as normal LogEntries, then `app.rs` will check if they should be routed to NetworkStore instead of LogStore.

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/parser/network.rs src/parser/mod.rs
git commit -m "feat(parser): add flog_net network protocol parser"
```

---

### Task 4: Add NetworkFilter

**Files:**
- Create: `src/domain/network_filter.rs`
- Modify: `src/domain/mod.rs`

- [ ] **Step 1: Create `src/domain/network_filter.rs`**

```rust
//! Filtering for network entries.

use regex::Regex;

use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Success2xx,
    Redirect3xx,
    ClientError4xx,
    ServerError5xx,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodFilter {
    All,
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolFilter {
    All,
    Http,
    Sse,
    Ws,
}

pub struct NetworkFilter {
    pub url_query: String,
    url_regex: Option<Regex>,
    pub protocol: ProtocolFilter,
    pub method: MethodFilter,
    pub status: StatusFilter,
    dirty: bool,
}

impl NetworkFilter {
    pub fn new() -> Self {
        Self {
            url_query: String::new(),
            url_regex: None,
            protocol: ProtocolFilter::All,
            method: MethodFilter::All,
            status: StatusFilter::All,
            dirty: false,
        }
    }

    pub fn set_url_query(&mut self, query: &str) {
        self.url_query = query.to_string();
        self.url_regex = if query.is_empty() {
            None
        } else {
            Regex::new(&format!("(?i){}", regex::escape(query))).ok()
        };
        self.dirty = true;
    }

    pub fn set_protocol(&mut self, p: ProtocolFilter) {
        self.protocol = p;
        self.dirty = true;
    }

    pub fn set_method(&mut self, m: MethodFilter) {
        self.method = m;
        self.dirty = true;
    }

    pub fn set_status(&mut self, s: StatusFilter) {
        self.status = s;
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    pub fn matches(&self, entry: &NetworkEntry) -> bool {
        // Protocol filter
        if self.protocol != ProtocolFilter::All {
            let matches = match self.protocol {
                ProtocolFilter::Http => entry.protocol == Protocol::Http,
                ProtocolFilter::Sse => entry.protocol == Protocol::Sse,
                ProtocolFilter::Ws => entry.protocol == Protocol::Ws,
                ProtocolFilter::All => true,
            };
            if !matches {
                return false;
            }
        }

        // Method filter
        if self.method != MethodFilter::All {
            let matches = match self.method {
                MethodFilter::Get => entry.method.eq_ignore_ascii_case("GET"),
                MethodFilter::Post => entry.method.eq_ignore_ascii_case("POST"),
                MethodFilter::Put => entry.method.eq_ignore_ascii_case("PUT"),
                MethodFilter::Delete => entry.method.eq_ignore_ascii_case("DELETE"),
                MethodFilter::Patch => entry.method.eq_ignore_ascii_case("PATCH"),
                MethodFilter::All => true,
            };
            if !matches {
                return false;
            }
        }

        // Status filter
        if self.status != StatusFilter::All {
            let matches = match self.status {
                StatusFilter::Success2xx => entry.http_status.map_or(false, |s| (200..300).contains(&s)),
                StatusFilter::Redirect3xx => entry.http_status.map_or(false, |s| (300..400).contains(&s)),
                StatusFilter::ClientError4xx => entry.http_status.map_or(false, |s| (400..500).contains(&s)),
                StatusFilter::ServerError5xx => entry.http_status.map_or(false, |s| s >= 500),
                StatusFilter::Failed => entry.status == NetworkStatus::Failed,
                StatusFilter::All => true,
            };
            if !matches {
                return false;
            }
        }

        // URL filter
        if let Some(re) = &self.url_regex {
            if !re.is_match(&entry.url) && !re.is_match(&entry.path) {
                return false;
            }
        }

        true
    }

    pub fn clear(&mut self) {
        self.url_query.clear();
        self.url_regex = None;
        self.protocol = ProtocolFilter::All;
        self.method = MethodFilter::All;
        self.status = StatusFilter::All;
        self.dirty = true;
    }
}
```

- [ ] **Step 2: Update `src/domain/mod.rs`**

Add module and re-exports:

```rust
pub mod entry;
pub mod filter;
pub mod network;
pub mod network_filter;
pub mod network_store;
pub mod store;

pub use entry::{InputSource, LogEntry, LogLevel, ParseResult};
pub use filter::FilterState;
pub use network::{
    FlogNetMessage, NetworkEntry, NetworkStatus, Protocol, SseChunk, WsDirection, WsMessage,
};
pub use network_filter::{MethodFilter, NetworkFilter, ProtocolFilter, StatusFilter};
pub use network_store::NetworkStore;
pub use store::LogStore;
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/domain/network_filter.rs src/domain/mod.rs
git commit -m "feat(domain): add NetworkFilter with protocol/method/status/url filtering"
```

---

### Task 5: Refactor App State — Add ViewTab and NetworkState

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Add ViewTab enum and NetworkState struct to app.rs**

At the top of `app.rs`, after the `AppMode` enum, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    Logs,
    Network,
}
```

Add `NetworkState` struct (minimal for now, will be expanded in Plan 3):

```rust
pub struct NetworkState {
    pub selected: usize,
    pub scroll_offset: usize,
    pub show_detail: bool,
    pub detail_scroll: usize,
    pub filter: crate::domain::NetworkFilter,
    filtered_indices: Vec<usize>,
    filter_dirty: bool,
}

impl NetworkState {
    pub fn new() -> Self {
        Self {
            selected: 0,
            scroll_offset: 0,
            show_detail: false,
            detail_scroll: 0,
            filter: crate::domain::NetworkFilter::new(),
            filtered_indices: Vec::new(),
            filter_dirty: true,
        }
    }

    pub fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    pub fn filtered_indices(&mut self, store: &crate::domain::NetworkStore) -> &[usize] {
        if self.filter_dirty {
            self.filtered_indices.clear();
            for i in 0..store.len() {
                if let Some(entry) = store.get(i) {
                    if self.filter.matches(entry) {
                        self.filtered_indices.push(i);
                    }
                }
            }
            self.filter_dirty = false;
        }
        &self.filtered_indices
    }

    pub fn filtered_count(&mut self, store: &crate::domain::NetworkStore) -> usize {
        self.filtered_indices(store).len()
    }
}
```

- [ ] **Step 2: Add `active_tab`, `network_state`, and `network_store` fields to App struct**

In the `App` struct, add these fields:

```rust
pub active_tab: ViewTab,
pub network_store: crate::domain::NetworkStore,
pub network: NetworkState,
```

- [ ] **Step 3: Initialize new fields in `App::new()` (or wherever App is constructed)**

In the App constructor, add:

```rust
active_tab: ViewTab::Logs,
network_store: crate::domain::NetworkStore::new(),
network: NetworkState::new(),
```

- [ ] **Step 4: Route flog_net entries to NetworkStore in `add_entry()`**

In `App::add_entry()`, before adding to LogStore, check if it's a network message:

```rust
pub fn add_entry(&mut self, entry: LogEntry) {
    // Check if this is a flog_net message that should go to NetworkStore
    if entry.tag == "flog_net" {
        if let Some(msg) = crate::parser::network::try_parse_network(&entry.tag, &entry.message) {
            self.network_store.process_message(msg);
            self.network.invalidate_filter();
            return;  // Don't add to LogStore
        }
    }

    // Normal log entry
    let drained = self.store.add_entry(entry);
    // ... existing drain/bookmark logic
}
```

Similarly in `add_raw_line()` — after parsing, before adding to store, check tag.

- [ ] **Step 5: Add tab switching methods**

```rust
pub fn switch_tab(&mut self, tab: ViewTab) {
    self.active_tab = tab;
}

pub fn next_tab(&mut self) {
    self.active_tab = match self.active_tab {
        ViewTab::Logs => ViewTab::Network,
        ViewTab::Network => ViewTab::Logs,
    };
}
```

- [ ] **Step 6: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add ViewTab, NetworkState, route flog_net to NetworkStore"
```

---

### Task 6: Move UI Files to `src/ui/logs/` Subdirectory

**Files:**
- Move: `src/ui/detail.rs` → `src/ui/logs/detail.rs`
- Move: `src/ui/highlight.rs` → `src/ui/logs/highlight.rs`
- Move: `src/ui/timeline.rs` → `src/ui/logs/timeline.rs`
- Move: `src/ui/stats.rs` → `src/ui/logs/stats.rs`
- Create: `src/ui/logs/mod.rs` (extract from `src/ui/mod.rs`)
- Modify: `src/ui/mod.rs` (become thin dispatcher)

This is the largest task — it restructures the UI layer. The key principle: move all Logs-specific rendering into `src/ui/logs/`, keep `src/ui/mod.rs` as a thin top-level dispatcher.

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p src/ui/logs src/ui/network
```

- [ ] **Step 2: Move files**

```bash
git mv src/ui/detail.rs src/ui/logs/detail.rs
git mv src/ui/highlight.rs src/ui/logs/highlight.rs
git mv src/ui/timeline.rs src/ui/logs/timeline.rs
git mv src/ui/stats.rs src/ui/logs/stats.rs
```

- [ ] **Step 3: Create `src/ui/logs/mod.rs`**

Move all Logs-specific code from `src/ui/mod.rs` into `src/ui/logs/mod.rs`:
- All palette constants (these are shared, so keep them in `src/ui/mod.rs` as `pub`)
- `level_color()`, `message_color()`, `level_badge()`, `level_pill()` → move to `src/ui/logs/mod.rs`
- `search_sparkline()`, `repeat_bar()`, `tag_pill_spans()`, `tag_color()` → move
- `draw_toolbar()`, `draw_status_bar()`, `draw_log_list()` → move
- `draw_not_connected()`, `draw_waiting_for_logs()`, `draw_no_matching_logs()` → move
- `highlight_with_filter()` → move
- `wrap_text()`, `safe_truncate()`, `safe_pad()` → keep in `src/ui/mod.rs` (shared utils)
- `entry_row_count_from_store()`, `apply_row_underline()` → move
- Logo constants, gradient functions → move

The `src/ui/logs/mod.rs` should declare its submodules:

```rust
pub mod detail;
pub mod highlight;
pub mod stats;
pub mod timeline;
```

And export a main `draw_logs()` function that renders the full Logs view (toolbar + list + timeline + status bar), similar to the current `draw()` function.

- [ ] **Step 4: Rewrite `src/ui/mod.rs` as dispatcher**

```rust
//! TUI rendering — top-level dispatcher.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};
use unicode_width::UnicodeWidthStr;

pub mod help;
pub mod logs;
pub mod network;
pub mod source_select;
mod tab_bar;

use crate::app::{App, AppMode, ViewTab};

// ══════════════════════════════════════
//  Shared Catppuccin Macchiato Palette
// ══════════════════════════════════════

pub const BASE: Color      = Color::Rgb(36, 39, 58);
pub const MANTLE: Color    = Color::Rgb(30, 32, 48);
pub const SURFACE0: Color  = Color::Rgb(54, 58, 79);
pub const SURFACE1: Color  = Color::Rgb(73, 77, 100);
pub const OVERLAY0: Color  = Color::Rgb(110, 115, 141);
pub const TEXT: Color       = Color::Rgb(202, 211, 245);
pub const SUBTEXT0: Color  = Color::Rgb(165, 173, 206);

pub const BLUE: Color      = Color::Rgb(138, 173, 244);
pub const SAPPHIRE: Color  = Color::Rgb(125, 196, 228);
pub const TEAL: Color      = Color::Rgb(139, 213, 202);
pub const GREEN: Color     = Color::Rgb(166, 218, 149);
pub const YELLOW: Color    = Color::Rgb(238, 212, 159);
pub const PEACH: Color     = Color::Rgb(245, 169, 127);
pub const RED: Color       = Color::Rgb(237, 135, 150);
pub const MAUVE: Color     = Color::Rgb(198, 160, 246);
pub const PINK: Color      = Color::Rgb(245, 189, 230);
pub const LAVENDER: Color  = Color::Rgb(183, 189, 248);

// ══════════════════════════════════════
//  Shared Utils
// ══════════════════════════════════════

// wrap_text, safe_truncate, safe_pad stay here as pub functions

// ══════════════════════════════════════
//  Main Draw
// ══════════════════════════════════════

pub fn draw(f: &mut Frame, app: &mut App) {
    app.tick += 1;

    let full = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BASE)), full);

    // SourceSelect takes full screen
    if app.mode == AppMode::SourceSelect {
        source_select::draw_source_select(f, app, full);
        return;
    }

    // Vertical: tab_bar | view content
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // tab bar
            Constraint::Min(3),    // view content
        ])
        .split(full);

    tab_bar::draw_tab_bar(f, app, rows[0]);

    match app.active_tab {
        ViewTab::Logs => logs::draw_logs(f, app, rows[1]),
        ViewTab::Network => network::draw_network(f, app, rows[1]),
    }
}
```

- [ ] **Step 5: Create `src/ui/tab_bar.rs`**

```rust
//! Tab bar renderer — switches between Logs and Network views.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, ViewTab};
use super::{MANTLE, BLUE, OVERLAY0, TEXT, SURFACE0};

pub fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    let logs_style = if app.active_tab == ViewTab::Logs {
        Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };

    let net_style = if app.active_tab == ViewTab::Network {
        Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };

    let mut spans = vec![
        Span::styled("  ", Style::default().bg(bg)),
        Span::styled(" Logs ", logs_style),
        Span::styled("    ", Style::default().bg(bg)),
        Span::styled(" Network ", net_style),
    ];

    // Fill remaining width
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    let rem = area.width as usize - used.min(area.width as usize);
    if rem > 0 {
        spans.push(Span::styled(" ".repeat(rem), Style::default().bg(bg)));
    }

    // Store click regions for mouse handling
    app.layout.tab_logs_x = (2, 8);    // " Logs " is at x=2..8
    app.layout.tab_network_x = (12, 21); // " Network " is at x=12..21

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

- [ ] **Step 6: Create placeholder `src/ui/network/mod.rs`**

```rust
//! Network Inspector view — placeholder until Plan 3.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::App;
use super::{BASE, OVERLAY0, SURFACE0, SURFACE1, BLUE};

pub fn draw_network(f: &mut Frame, app: &mut App, area: Rect) {
    let mid_y = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();

    for _ in 0..mid_y.saturating_sub(2) {
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(Span::styled(
        "    Network Inspector",
        Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "    Add FlogHttpInterceptor to your Dio instance",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::from(Span::styled(
        "    to see network requests here.",
        Style::default().fg(SURFACE1),
    )));

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(BASE)),
        area,
    );
}
```

- [ ] **Step 7: Add tab click regions to LayoutCache**

In `src/app.rs`, add to the `LayoutCache` struct:

```rust
pub tab_logs_x: (u16, u16),
pub tab_network_x: (u16, u16),
pub tab_bar_y: u16,
```

Initialize them to `(0, 0)` / `0` in the LayoutCache constructor.

- [ ] **Step 8: Add tab click handling to event.rs**

In `handle_mouse()`, before dispatching to view-specific handlers, check for tab bar clicks:

```rust
// Tab bar click
if mouse.row == app.layout.tab_bar_y {
    let x = mouse.column;
    if x >= app.layout.tab_logs_x.0 && x < app.layout.tab_logs_x.1 {
        app.switch_tab(ViewTab::Logs);
        return;
    }
    if x >= app.layout.tab_network_x.0 && x < app.layout.tab_network_x.1 {
        app.switch_tab(ViewTab::Network);
        return;
    }
}
```

Add keyboard shortcuts for tab switching in Normal mode:

```rust
KeyCode::Char('1') => app.switch_tab(ViewTab::Logs),
KeyCode::Char('2') => app.switch_tab(ViewTab::Network),
```

- [ ] **Step 9: Update all internal module paths**

After the move, update `use` statements in the moved files:
- `src/ui/logs/detail.rs`: change `use crate::app::App` paths (should work without changes if they use `crate::`)
- `src/ui/logs/highlight.rs`: no changes needed (uses `ratatui` and `regex` only)
- `src/ui/logs/timeline.rs`: change `use crate::app::App` (should already work)
- `src/ui/logs/stats.rs`: change `use crate::app::App` (should already work)
- `src/ui/logs/mod.rs`: import palette colors from `super::` instead of defining them locally

- [ ] **Step 10: Build and fix all compilation errors**

Run: `cargo build 2>&1`

Fix any path/import errors. This is the most complex step — expect to iterate on import paths.

- [ ] **Step 11: Run tests**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "refactor(ui): split into logs/ and network/ modules, add tab bar"
```

---

### Task 7: Update main.rs Event Loop for Tab-Aware Rendering

**Files:**
- Modify: `src/main.rs` (event loop, around lines 413-451)

- [ ] **Step 1: Update rendering dispatch**

The current event loop renders Help and Stats as overlays. Update to use the new `ui::draw()` which already handles tab dispatching:

```rust
terminal.draw(|f| {
    let mut app = app_lock.lock().unwrap();
    match app.mode {
        AppMode::Help => ui::help::draw_help(f),
        AppMode::Stats => ui::stats::draw_stats(f, &mut app),
        _ => ui::draw(f, &mut app),
    }
})?;
```

This should already match the current code structure. Verify the Help and Stats overlays still reference correct paths after the move (`ui::help` and `ui::logs::stats` or `ui::stats`).

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): update event loop for tab-aware rendering"
```

---

### Task 8: Update Session Persistence for ViewTab

**Files:**
- Modify: `src/session.rs`

- [ ] **Step 1: Add active_tab to SessionData**

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct SessionData {
    pub min_level: u8,
    pub tag_filter_input: String,
    pub search_query: String,
    pub bookmarks: Vec<usize>,
    pub active_tab: u8,  // 0 = Logs, 1 = Network
}
```

- [ ] **Step 2: Save and load active_tab**

In `save_session()`:

```rust
active_tab: match app.active_tab {
    crate::app::ViewTab::Logs => 0,
    crate::app::ViewTab::Network => 1,
},
```

In `load_session()`:

```rust
app.active_tab = match data.active_tab {
    1 => crate::app::ViewTab::Network,
    _ => crate::app::ViewTab::Logs,
};
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/session.rs
git commit -m "feat(session): persist active tab across sessions"
```

---

### Task 9: Final Build and Test

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | head -40`
Expected: No errors.

- [ ] **Step 3: Run fmt**

Run: `cargo fmt -- --check 2>&1`
Expected: No formatting issues.

- [ ] **Step 4: Build release**

Run: `cargo build --release 2>&1 | head -10`
Expected: Compiles successfully.

- [ ] **Step 5: Manual smoke test**

Run the binary and verify:
- Tab bar appears at top with Logs/Network tabs
- Clicking Logs/Network switches views
- Pressing 1/2 switches views
- Logs view works identically to before (with Plan 1 visual improvements)
- Network view shows placeholder message
