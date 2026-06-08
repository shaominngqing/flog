# DevTools Network Timing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add DevTools-grade timing details for HTTP, SSE, and WebSocket entries in the Network detail panel without changing the Network list.

**Architecture:** Add a pure Rust `network_timing` domain model and optional wire fields, ingest timing in `NetworkStore`, render a protocol-specific `Timing` detail section, and extend `flog_dart` with a shared timing core plus HTTP/SSE/WS collectors. All protocol changes are additive and old clients keep working.

**Tech Stack:** Rust, serde, ratatui, cargo tests; Dart/Flutter, Dio `HttpClientAdapter`, `Stream`, `web_socket_channel`, `flutter_test`.

---

## File Structure

**Rust domain/protocol**

- Create `src/domain/network_timing.rs`: pure timing data types, serde mappings, helper formatting-free methods.
- Modify `src/domain/mod.rs`: expose timing types if needed by UI/tests.
- Modify `src/domain/network.rs`: add `NetworkEntry.timing`, `SseChunk.event_timing`, `WsMessage.event_timing`, and optional timing fields on `FlogNetKind`.
- Modify `src/domain/network_tests.rs`: protocol deserialize and type default tests.
- Modify `src/domain/network_store.rs`: store full trace timing and event timing.
- Modify `src/domain/network_store_tests.rs`: ingestion tests.

**Rust UI**

- Create `src/ui/network/detail/timing.rs`: protocol-specific Timing section renderer and small pure helpers.
- Modify `src/ui/network/detail/mod.rs`: insert Timing after General.
- Modify `src/ui/network/detail/shared.rs` only if a reusable pill/kv helper is needed; keep changes small.

**Dart timing**

- Create `flog_dart/lib/src/timing/timing_trace.dart`: shared timing trace model and JSON conversion.
- Create `flog_dart/lib/src/timing/timing_clock.dart`: monotonic microsecond clock abstraction for tests.
- Create `flog_dart/lib/src/timing/timing_stream.dart`: stream tee that records first byte, byte count, gaps, completion, and errors.
- Create `flog_dart/lib/src/timing/timing_adapter.dart`: `FlogTimingHttpClientAdapter` and custom-adapter wrapper.
- Modify `flog_dart/lib/src/flog_dio.dart`: install/wrap timing adapter by default.
- Modify `flog_dart/lib/src/flog_http_interceptor.dart`: attach existing request IDs to timing traces and emit timing in `res/err`.
- Modify `flog_dart/lib/src/flog_mock_interceptor.dart`: stamp mock delay timing metadata.
- Modify `flog_dart/lib/src/sse/reporter.dart`: emit `eventTiming` for chunks and full timing on done/error.
- Modify `flog_dart/lib/src/flog_web_socket.dart`: emit full WS timing and per-message `eventTiming`.

**Dart tests**

- Create `flog_dart/test/timing/timing_trace_test.dart`.
- Create `flog_dart/test/timing/timing_stream_test.dart`.
- Create `flog_dart/test/timing/timing_adapter_test.dart`.
- Modify `flog_dart/test/flog_http_interceptor_test.dart`.
- Modify `flog_dart/test/sse/reporter_test.dart`.
- Modify `flog_dart/test/flog_web_socket_test.dart`.

**Docs**

- Modify `docs/PROTOCOL.md`: timing wire schema.
- Modify `docs/MODULES.md`: new timing modules.
- Optionally modify `flog_dart/README.md` only if public behavior needs a short user-facing note.

---

## Task 1: Rust Timing Domain and Wire Types

**Files:**
- Create: `src/domain/network_timing.rs`
- Modify: `src/domain/mod.rs`
- Modify: `src/domain/network.rs`
- Test: `src/domain/network_tests.rs`
- Test: `src/input/protocol_tests.rs`

- [ ] **Step 1: Write failing deserialize tests for full timing**

Add to `src/domain/network_tests.rs`:

```rust
#[test]
fn network_timing_deserializes_full_trace() {
    let json = r#"{
        "v": 1,
        "source": "flog_adapter",
        "clock": "monotonic_us",
        "startUs": 0,
        "endUs": 126000,
        "connection": {
            "id": "https://api.example.com:443#3",
            "reused": false,
            "protocol": "http/1.1"
        },
        "phases": [
            {
                "name": "ttfb",
                "startUs": 62000,
                "endUs": 104000,
                "status": "complete",
                "confidence": "exact",
                "detail": "headers to first byte"
            }
        ],
        "events": [
            {"name": "headers", "atUs": 62000},
            {"name": "first_byte", "atUs": 104000, "gapUs": 42000, "size": 1}
        ],
        "notes": ["TLS boundary approximated by adapter"]
    }"#;

    let timing: crate::domain::network_timing::NetworkTiming =
        serde_json::from_str(json).expect("timing should deserialize");

    assert_eq!(timing.version, 1);
    assert_eq!(timing.source, crate::domain::network_timing::TimingSource::FlogAdapter);
    assert_eq!(timing.clock, crate::domain::network_timing::TimingClock::MonotonicUs);
    assert_eq!(timing.connection.as_ref().unwrap().id.as_deref(), Some("https://api.example.com:443#3"));
    assert!(!timing.connection.as_ref().unwrap().reused);
    assert_eq!(timing.phases[0].name, "ttfb");
    assert_eq!(timing.phases[0].confidence, crate::domain::network_timing::TimingConfidence::Exact);
    assert_eq!(timing.events[1].gap_us, Some(42_000));
    assert_eq!(timing.notes, vec!["TLS boundary approximated by adapter"]);
}
```

- [ ] **Step 2: Write failing tests for optional timing on `FlogNetKind`**

Add to `src/domain/network_tests.rs`:

```rust
#[test]
fn flog_net_kind_res_accepts_optional_timing() {
    let json = r#"{
        "t": "res",
        "id": 42,
        "status": 200,
        "duration": 126,
        "timing": {
            "v": 1,
            "source": "flog_adapter",
            "clock": "monotonic_us",
            "startUs": 0,
            "endUs": 126000,
            "phases": [],
            "events": [],
            "notes": []
        }
    }"#;

    let msg: FlogNetKind = serde_json::from_str(json).expect("res should deserialize");
    match msg {
        FlogNetKind::Res { timing, .. } => {
            assert!(timing.is_some());
            assert_eq!(timing.unwrap().end_us, Some(126_000));
        }
        other => panic!("expected res, got {:?}", other),
    }
}

#[test]
fn flog_net_kind_chunk_accepts_event_timing() {
    let json = r#"{
        "t": "chunk",
        "id": 2,
        "data": "payload",
        "size": 208,
        "seq": 4,
        "eventTiming": {"name": "chunk", "atUs": 1259000, "gapUs": 812000, "size": 208}
    }"#;

    let msg: FlogNetKind = serde_json::from_str(json).expect("chunk should deserialize");
    match msg {
        FlogNetKind::Chunk { event_timing, .. } => {
            let event = event_timing.expect("event timing should be present");
            assert_eq!(event.at_us, 1_259_000);
            assert_eq!(event.gap_us, Some(812_000));
            assert_eq!(event.size, Some(208));
        }
        other => panic!("expected chunk, got {:?}", other),
    }
}
```

- [ ] **Step 3: Run the failing Rust tests**

Run:

```bash
cargo test domain::network_tests::network_timing_deserializes_full_trace domain::network_tests::flog_net_kind_res_accepts_optional_timing domain::network_tests::flog_net_kind_chunk_accepts_event_timing
```

Expected: FAIL with unresolved `network_timing`, missing `timing`, and missing `event_timing` fields.

- [ ] **Step 4: Add `src/domain/network_timing.rs`**

Create the file with:

```rust
//! Pure network timing data for HTTP, SSE, and WebSocket detail views.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingSource {
    FlogAdapter,
    Interceptor,
    SseReporter,
    WsWrapper,
    CustomAdapter,
    NativeHook,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingClock {
    MonotonicUs,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingPhaseStatus {
    Complete,
    Active,
    Unavailable,
    Reused,
    Skipped,
    Cancelled,
    Errored,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimingConfidence {
    Exact,
    Approx,
    Inferred,
    Unavailable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingConnection {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub reused: bool,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingPhase {
    pub name: String,
    #[serde(default)]
    pub start_us: Option<u64>,
    #[serde(default)]
    pub end_us: Option<u64>,
    #[serde(default = "TimingPhase::default_status")]
    pub status: TimingPhaseStatus,
    #[serde(default = "TimingPhase::default_confidence")]
    pub confidence: TimingConfidence,
    #[serde(default)]
    pub detail: Option<String>,
}

impl TimingPhase {
    fn default_status() -> TimingPhaseStatus {
        TimingPhaseStatus::Complete
    }

    fn default_confidence() -> TimingConfidence {
        TimingConfidence::Exact
    }

    pub fn duration_us(&self) -> Option<u64> {
        Some(self.end_us?.saturating_sub(self.start_us?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingEvent {
    #[serde(default = "TimingEvent::default_name")]
    pub name: String,
    pub at_us: u64,
    #[serde(default)]
    pub gap_us: Option<u64>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub detail: Option<String>,
}

impl TimingEvent {
    fn default_name() -> String {
        "event".to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTiming {
    #[serde(rename = "v")]
    pub version: u16,
    pub source: TimingSource,
    pub clock: TimingClock,
    #[serde(default)]
    pub start_us: Option<u64>,
    #[serde(default)]
    pub end_us: Option<u64>,
    #[serde(default)]
    pub connection: Option<TimingConnection>,
    #[serde(default)]
    pub phases: Vec<TimingPhase>,
    #[serde(default)]
    pub events: Vec<TimingEvent>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl NetworkTiming {
    pub fn total_duration_us(&self) -> Option<u64> {
        Some(self.end_us?.saturating_sub(self.start_us?))
    }
}
```

- [ ] **Step 5: Wire timing types into `domain` and `network.rs`**

In `src/domain/mod.rs`, add:

```rust
pub mod network_timing;
```

In `src/domain/network.rs`, import timing:

```rust
use crate::domain::network_timing::{NetworkTiming, TimingEvent};
```

Add fields:

```rust
pub struct SseChunk {
    pub data: String,
    pub event_timing: Option<TimingEvent>,
}

pub struct WsMessage {
    pub direction: WsDirection,
    pub data: String,
    pub size: u64,
    pub event_timing: Option<TimingEvent>,
}

pub struct NetworkEntry {
    ...
    pub timing: Option<NetworkTiming>,
}
```

Add `timing: None` to every `NetworkEntry` constructor/default literal.

Add `timing` to terminal variants and `event_timing` to event variants:

```rust
Res {
    id: u64,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    duration: Option<u64>,
    #[serde(default)]
    headers: Option<serde_json::Value>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    mocked: Option<bool>,
    #[serde(default)]
    timing: Option<NetworkTiming>,
    #[serde(default)]
    ts: Option<u64>,
}
```

For `Chunk`, `Send`, and `Recv`, use wire name `eventTiming`:

```rust
#[serde(default, rename = "eventTiming")]
event_timing: Option<TimingEvent>,
```

- [ ] **Step 6: Update existing test factories to compile**

In `src/domain/network_store_tests.rs`, add `timing: None` to `Res`, `Err`, `Done`, `Open`, `Connecting`, and `Close` factories/constructors. Add `event_timing: None` to `Chunk`, `Send`, and `Recv` factories/constructors.

Example:

```rust
fn chunk(id: u64, data: Option<&str>) -> FlogNetKind {
    FlogNetKind::Chunk {
        id,
        data: data.map(|s| s.to_string()),
        size: None,
        seq: None,
        event_timing: None,
        ts: None,
    }
}
```

- [ ] **Step 7: Run timing domain tests**

Run:

```bash
cargo test network_timing_deserializes_full_trace flog_net_kind_res_accepts_optional_timing flog_net_kind_chunk_accepts_event_timing
```

Expected: PASS.

- [ ] **Step 8: Run full Rust domain network tests**

Run:

```bash
cargo test domain::network_tests domain::network_store_tests
```

Expected: PASS.

- [ ] **Step 9: Commit Task 1**

```bash
git add src/domain/network_timing.rs src/domain/mod.rs src/domain/network.rs src/domain/network_tests.rs src/domain/network_store_tests.rs src/input/protocol_tests.rs
git commit -m "feat: add network timing protocol types"
```

---

## Task 2: Rust NetworkStore Timing Ingestion

**Files:**
- Modify: `src/domain/network_store.rs`
- Test: `src/domain/network_store_tests.rs`

- [ ] **Step 1: Write failing store tests for full timing and event timing**

Add to `src/domain/network_store_tests.rs`:

```rust
fn sample_timing() -> crate::domain::network_timing::NetworkTiming {
    serde_json::from_value(serde_json::json!({
        "v": 1,
        "source": "flog_adapter",
        "clock": "monotonic_us",
        "startUs": 0,
        "endUs": 126000,
        "phases": [
            {"name": "ttfb", "startUs": 62000, "endUs": 104000, "status": "complete", "confidence": "exact"}
        ],
        "events": [],
        "notes": []
    }))
    .expect("sample timing")
}

fn event_timing(name: &str, at_us: u64, gap_us: Option<u64>) -> crate::domain::network_timing::TimingEvent {
    crate::domain::network_timing::TimingEvent {
        name: name.to_string(),
        at_us,
        gap_us,
        size: None,
        detail: None,
    }
}

#[test]
fn handle_res_stores_network_timing() {
    let mut store = NetworkStore::new();
    store.process_message(req_http(1, "GET", "https://x.com"));
    store.process_message(FlogNetKind::Res {
        id: 1,
        status: Some(200),
        duration: Some(126),
        headers: None,
        body: None,
        size: None,
        error: None,
        mocked: None,
        timing: Some(sample_timing()),
        ts: None,
    });

    let entry = store.get(0).unwrap();
    assert_eq!(entry.timing.as_ref().unwrap().end_us, Some(126_000));
    assert_eq!(entry.timing.as_ref().unwrap().phases[0].name, "ttfb");
}

#[test]
fn handle_chunk_stores_event_timing() {
    let mut store = NetworkStore::new();
    store.process_message(req_sse(1, "GET", "https://x.com/sse"));
    store.process_message(FlogNetKind::Chunk {
        id: 1,
        data: Some("hello".into()),
        size: Some(5),
        seq: Some(1),
        event_timing: Some(event_timing("chunk", 421_000, Some(421_000))),
        ts: None,
    });

    let chunk = &store.get(0).unwrap().sse_chunks[0];
    assert_eq!(chunk.event_timing.as_ref().unwrap().at_us, 421_000);
    assert_eq!(chunk.event_timing.as_ref().unwrap().gap_us, Some(421_000));
}

#[test]
fn handle_ws_messages_store_event_timing() {
    let mut store = NetworkStore::new();
    store.process_message(open(1, "wss://x.com/ws"));
    store.process_message(FlogNetKind::Recv {
        id: 1,
        data: Some("pong".into()),
        size: Some(4),
        event_timing: Some(event_timing("recv", 244_000, Some(62_000))),
        ts: None,
    });

    let msg = &store.get(0).unwrap().ws_messages[0];
    assert_eq!(msg.event_timing.as_ref().unwrap().at_us, 244_000);
    assert_eq!(msg.event_timing.as_ref().unwrap().gap_us, Some(62_000));
}
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
cargo test handle_res_stores_network_timing handle_chunk_stores_event_timing handle_ws_messages_store_event_timing
```

Expected: FAIL because `NetworkStore` drops timing fields.

- [ ] **Step 3: Thread timing through `process_message`**

In `src/domain/network_store.rs`, update match arms:

```rust
FlogNetKind::Res {
    id,
    status,
    duration,
    headers,
    body,
    size,
    error,
    mocked,
    timing,
    ts: _,
} => self.handle_res(id, status, duration, headers, body, size, error, mocked, timing),
```

Update `handle_res` signature:

```rust
fn handle_res(
    &mut self,
    id: u64,
    status: Option<u16>,
    duration: Option<u64>,
    headers: Option<serde_json::Value>,
    body: Option<String>,
    size: Option<u64>,
    error: Option<String>,
    mocked: Option<bool>,
    timing: Option<crate::domain::network_timing::NetworkTiming>,
)
```

Set:

```rust
entry.timing = timing;
```

When creating orphan responses, set `entry.timing = timing;` before pushing.

- [ ] **Step 4: Store event timing on SSE and WS messages**

Update `handle_chunk`:

```rust
fn handle_chunk(
    &mut self,
    id: u64,
    data: Option<String>,
    size: Option<u64>,
    event_timing: Option<crate::domain::network_timing::TimingEvent>,
) {
    if let Some(entry) = self.find_by_id_mut(id) {
        let data = data.unwrap_or_default();
        let chunk_size = size.unwrap_or(data.len() as u64);
        entry.sse_total_size += chunk_size;
        entry.sse_chunks.push(SseChunk { data, event_timing });
    }
}
```

Update `handle_ws_msg`:

```rust
fn handle_ws_msg(
    &mut self,
    id: u64,
    data: Option<String>,
    size: Option<u64>,
    direction: WsDirection,
    event_timing: Option<crate::domain::network_timing::TimingEvent>,
) {
    if let Some(entry) = self.find_by_id_mut(id) {
        let data = data.unwrap_or_default();
        let msg_size = size.unwrap_or(data.len() as u64);
        entry.ws_messages.push(WsMessage {
            direction,
            data,
            size: msg_size,
            event_timing,
        });
    }
}
```

- [ ] **Step 5: Store full timing on `Err`, `Done`, `Open`, and `Close`**

Update handlers so terminal timing is not HTTP-only:

```rust
if let Some(entry) = self.find_by_id_mut(id) {
    entry.timing = timing;
}
```

Apply this in `handle_err`, `handle_done`, `handle_open`, and `handle_close` by adding a `timing: Option<NetworkTiming>` parameter to each handler.

- [ ] **Step 6: Run store timing tests**

Run:

```bash
cargo test handle_res_stores_network_timing handle_chunk_stores_event_timing handle_ws_messages_store_event_timing
```

Expected: PASS.

- [ ] **Step 7: Run all Rust tests affected by network constructors**

Run:

```bash
cargo test --all
```

Expected: PASS. If constructor fallout appears, update the failing struct literals by adding `timing: None` or `event_timing: None` without changing behavior.

- [ ] **Step 8: Commit Task 2**

```bash
git add src/domain/network_store.rs src/domain/network_store_tests.rs src/domain/network.rs src/domain/network_tests.rs
git commit -m "feat: store network timing data"
```

---

## Task 3: Rust Timing Detail Renderer

**Files:**
- Create: `src/ui/network/detail/timing.rs`
- Modify: `src/ui/network/detail/mod.rs`
- Test: `src/ui/network/detail/timing.rs`

- [ ] **Step 1: Write pure helper tests for formatting and bottleneck selection**

At the bottom of `src/ui/network/detail/timing.rs`, add tests while creating the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::network_timing::{
        TimingConfidence, TimingPhase, TimingPhaseStatus,
    };

    fn phase(name: &str, start_us: u64, end_us: u64) -> TimingPhase {
        TimingPhase {
            name: name.to_string(),
            start_us: Some(start_us),
            end_us: Some(end_us),
            status: TimingPhaseStatus::Complete,
            confidence: TimingConfidence::Exact,
            detail: None,
        }
    }

    #[test]
    fn format_us_uses_ms_and_seconds() {
        assert_eq!(format_us(999), "999us");
        assert_eq!(format_us(1_000), "1ms");
        assert_eq!(format_us(126_000), "126ms");
        assert_eq!(format_us(1_500_000), "1.5s");
    }

    #[test]
    fn bottleneck_picks_longest_complete_phase() {
        let phases = vec![
            phase("dns", 0, 7_000),
            phase("ttfb", 62_000, 104_000),
            phase("decode", 104_000, 112_000),
        ];
        let found = bottleneck_phase(&phases).expect("bottleneck");
        assert_eq!(found.name, "ttfb");
    }

    #[test]
    fn bar_width_is_bounded() {
        assert_eq!(bar_cells(0, 100, 20), 1);
        assert_eq!(bar_cells(50, 100, 20), 10);
        assert_eq!(bar_cells(100, 100, 20), 20);
    }
}
```

- [ ] **Step 2: Run the failing helper tests**

Run:

```bash
cargo test ui::network::detail::timing::tests
```

Expected: FAIL because helpers are missing.

- [ ] **Step 3: Implement the timing renderer shell and helpers**

Create `src/ui/network/detail/timing.rs` with:

```rust
//! Network detail Timing section renderers.

use std::collections::HashSet;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::domain::network::{NetworkEntry, Protocol};
use crate::domain::network_timing::{NetworkTiming, TimingPhase};
use crate::ui::json_viewer::JsonHotRegion;

use super::shared::push_section_header;
use super::KEY_COLOR;
use crate::ui::{GREEN, MAUVE, OVERLAY0, PEACH, SAPPHIRE, SUBTEXT0, TEAL, YELLOW, MANTLE, TEXT};

pub(super) fn render_timing(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    collapsed_sections: &HashSet<String>,
    inner_w: usize,
) {
    if entry.timing.is_none()
        && entry.sse_chunks.iter().all(|c| c.event_timing.is_none())
        && entry.ws_messages.iter().all(|m| m.event_timing.is_none())
    {
        return;
    }

    let sec = "Timing";
    let collapsed = collapsed_sections.contains(sec);
    push_section_header(lines, section_map, json_click_map, json_section_keys, sec, collapsed);
    if collapsed {
        return;
    }

    match entry.protocol {
        Protocol::Http => render_http(lines, section_map, json_click_map, json_section_keys, entry, inner_w),
        Protocol::Sse => render_sse(lines, section_map, json_click_map, json_section_keys, entry, inner_w),
        Protocol::Ws => render_ws(lines, section_map, json_click_map, json_section_keys, entry, inner_w),
    }
}

fn push_plain(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    line: Line<'static>,
) {
    lines.push(line);
    section_map.push(None);
    json_click_map.push(Vec::new());
    json_section_keys.push(None);
}

fn format_us(us: u64) -> String {
    if us >= 1_000_000 {
        format!("{:.1}s", us as f64 / 1_000_000.0)
    } else if us >= 1_000 {
        format!("{}ms", us / 1_000)
    } else {
        format!("{}us", us)
    }
}

fn bottleneck_phase(phases: &[TimingPhase]) -> Option<&TimingPhase> {
    phases.iter().max_by_key(|p| p.duration_us().unwrap_or(0))
}

fn bar_cells(value: u64, total: u64, max_w: usize) -> usize {
    if max_w == 0 {
        return 0;
    }
    if total == 0 || value == 0 {
        return 1;
    }
    ((value as f64 / total as f64) * max_w as f64).ceil() as usize
}
```

- [ ] **Step 4: Implement protocol-specific render functions**

Add these functions to the same file:

```rust
fn render_http(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    inner_w: usize,
) {
    let Some(timing) = entry.timing.as_ref() else { return; };
    let total_us = timing.total_duration_us().or_else(|| entry.duration.map(|d| d * 1_000));
    let bottleneck = bottleneck_phase(&timing.phases);
    let bottleneck_text = bottleneck
        .and_then(|p| p.duration_us().map(|d| format!("{} {}", p.name, format_us(d))))
        .unwrap_or_else(|| "-".to_string());

    push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(vec![
        Span::styled("   Total: ", Style::default().fg(KEY_COLOR)),
        Span::styled(total_us.map(format_us).unwrap_or_else(|| "-".to_string()), Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled("Bottleneck: ", Style::default().fg(KEY_COLOR)),
        Span::styled(bottleneck_text, Style::default().fg(PEACH).add_modifier(Modifier::BOLD)),
    ]));

    push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(Span::styled(
        "   Phase              Time    Trust       Waterfall",
        Style::default().fg(OVERLAY0),
    )));

    let max_bar_w = inner_w.saturating_sub(42).min(28);
    let total = total_us.unwrap_or(0).max(1);
    for phase in &timing.phases {
        let dur = phase.duration_us().unwrap_or(0);
        let cells = bar_cells(dur, total, max_bar_w);
        let bar = "█".repeat(cells);
        push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(vec![
            Span::raw(format!("   {:<17}", phase.name)),
            Span::styled(format!("{:>7}", format_us(dur)), Style::default().fg(TEAL)),
            Span::raw("   "),
            Span::styled(format!("{:<10}", format!("{:?}", phase.confidence).to_lowercase()), Style::default().fg(GREEN)),
            Span::styled(bar, Style::default().fg(PEACH)),
        ]));
    }

    push_notes(lines, section_map, json_click_map, json_section_keys, timing);
}

fn render_sse(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    _inner_w: usize,
) {
    let event_count = entry.sse_chunks.len();
    let worst_gap = entry
        .sse_chunks
        .iter()
        .filter_map(|c| c.event_timing.as_ref()?.gap_us)
        .max();
    push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(vec![
        Span::styled("   Events: ", Style::default().fg(KEY_COLOR)),
        Span::styled(event_count.to_string(), Style::default().fg(TEAL)),
        Span::raw("   "),
        Span::styled("Worst gap: ", Style::default().fg(KEY_COLOR)),
        Span::styled(worst_gap.map(format_us).unwrap_or_else(|| "-".to_string()), Style::default().fg(PEACH).add_modifier(Modifier::BOLD)),
    ]));

    push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(Span::styled(
        "   Event gaps",
        Style::default().fg(OVERLAY0),
    )));
    for (idx, chunk) in entry.sse_chunks.iter().enumerate().take(8) {
        if let Some(event) = &chunk.event_timing {
            push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(vec![
                Span::styled(format!("   #{:03} ", idx + 1), Style::default().fg(TEXT).bg(crate::ui::SURFACE0)),
                Span::styled(format!("{:>8}", event.gap_us.map(format_us).unwrap_or_else(|| "-".to_string())), Style::default().fg(TEAL)),
                Span::raw("  "),
                Span::styled(format!("{}B", chunk.data.len()), Style::default().fg(SUBTEXT0)),
            ]));
        }
    }
    if let Some(timing) = &entry.timing {
        push_notes(lines, section_map, json_click_map, json_section_keys, timing);
    }
}

fn render_ws(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    entry: &NetworkEntry,
    _inner_w: usize,
) {
    let worst_gap = entry
        .ws_messages
        .iter()
        .filter_map(|m| m.event_timing.as_ref()?.gap_us)
        .max();
    push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(vec![
        Span::styled("   Messages: ", Style::default().fg(KEY_COLOR)),
        Span::styled(entry.ws_messages.len().to_string(), Style::default().fg(TEAL)),
        Span::raw("   "),
        Span::styled("Worst idle: ", Style::default().fg(KEY_COLOR)),
        Span::styled(worst_gap.map(format_us).unwrap_or_else(|| "-".to_string()), Style::default().fg(PEACH).add_modifier(Modifier::BOLD)),
    ]));

    push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(Span::styled(
        "   Message timeline",
        Style::default().fg(OVERLAY0),
    )));
    for msg in entry.ws_messages.iter().take(8) {
        let label = match msg.direction {
            crate::domain::network::WsDirection::Send => " SEND ",
            crate::domain::network::WsDirection::Recv => " RECV ",
        };
        push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(vec![
            Span::styled(label, Style::default().fg(MANTLE).bg(MAUVE).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(format!("{:>8}", msg.event_timing.as_ref().map(|e| format_us(e.at_us)).unwrap_or_else(|| "-".to_string())), Style::default().fg(SUBTEXT0)),
            Span::raw(" "),
            Span::styled(format!("{:>5}B", msg.size), Style::default().fg(SUBTEXT0)),
        ]));
    }
    if let Some(timing) = &entry.timing {
        push_notes(lines, section_map, json_click_map, json_section_keys, timing);
    }
}

fn push_notes(
    lines: &mut Vec<Line<'static>>,
    section_map: &mut Vec<Option<String>>,
    json_click_map: &mut Vec<Vec<JsonHotRegion>>,
    json_section_keys: &mut Vec<Option<String>>,
    timing: &NetworkTiming,
) {
    for note in &timing.notes {
        push_plain(lines, section_map, json_click_map, json_section_keys, Line::from(vec![
            Span::styled("   notes: ", Style::default().fg(OVERLAY0)),
            Span::styled(note.clone(), Style::default().fg(SUBTEXT0)),
        ]));
    }
}
```

- [ ] **Step 5: Insert Timing section in detail renderer**

In `src/ui/network/detail/mod.rs`, add module:

```rust
mod timing;
```

After `general::render_general(...)`, insert:

```rust
timing::render_timing(
    &mut all_lines,
    &mut section_line_map,
    &mut json_click_map,
    &mut json_section_keys,
    &entry,
    &app.network.collapsed_sections,
    inner_w,
);
```

- [ ] **Step 6: Run UI timing tests**

Run:

```bash
cargo test ui::network::detail::timing::tests
```

Expected: PASS.

- [ ] **Step 7: Run full Rust verification**

Run:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all
```

Expected: all PASS. If `cargo fmt -- --check` fails only due formatting, run `cargo fmt` and re-run the check.

- [ ] **Step 8: Commit Task 3**

```bash
git add src/ui/network/detail/timing.rs src/ui/network/detail/mod.rs
git commit -m "feat: render network timing details"
```

---

## Task 4: Dart Timing Core

**Files:**
- Create: `flog_dart/lib/src/timing/timing_trace.dart`
- Create: `flog_dart/lib/src/timing/timing_clock.dart`
- Test: `flog_dart/test/timing/timing_trace_test.dart`

- [ ] **Step 1: Write failing Dart model tests**

Create `flog_dart/test/timing/timing_trace_test.dart`:

```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:flog_dart/src/timing/timing_trace.dart';

void main() {
  group('FlogTimingTrace', () {
    test('serializes using wire field names', () {
      final trace = FlogTimingTrace(
        version: 1,
        source: 'flog_adapter',
        clock: 'monotonic_us',
        startUs: 0,
        endUs: 126000,
        connection: const FlogTimingConnection(
          id: 'https://api.example.com:443#3',
          reused: false,
          protocol: 'http/1.1',
        ),
        phases: const [
          FlogTimingPhase(
            name: 'ttfb',
            startUs: 62000,
            endUs: 104000,
            status: 'complete',
            confidence: 'exact',
          ),
        ],
        events: const [
          FlogTimingEvent(name: 'first_byte', atUs: 104000, gapUs: 42000, size: 1),
        ],
        notes: const ['TLS boundary approximated by adapter'],
      );

      final json = trace.toJson();
      expect(json['v'], 1);
      expect(json['source'], 'flog_adapter');
      expect(json['clock'], 'monotonic_us');
      expect(json['startUs'], 0);
      expect(json['endUs'], 126000);
      expect(json['connection']['id'], 'https://api.example.com:443#3');
      expect(json['phases'][0]['name'], 'ttfb');
      expect(json['events'][0]['gapUs'], 42000);
      expect(json['notes'], ['TLS boundary approximated by adapter']);
    });

    test('durationUs is null until endUs is present', () {
      final trace = FlogTimingTrace(
        version: 1,
        source: 'ws_wrapper',
        clock: 'monotonic_us',
        startUs: 10,
        phases: const [],
        events: const [],
        notes: const [],
      );
      expect(trace.durationUs, isNull);
      expect(trace.finish(30).durationUs, 20);
    });
  });
}
```

- [ ] **Step 2: Run failing Dart model tests**

Run:

```bash
cd flog_dart && flutter test test/timing/timing_trace_test.dart
```

Expected: FAIL because timing model files do not exist.

- [ ] **Step 3: Implement `timing_trace.dart`**

Create `flog_dart/lib/src/timing/timing_trace.dart`:

```dart
class FlogTimingConnection {
  final String? id;
  final bool reused;
  final String? protocol;
  final String? proxy;

  const FlogTimingConnection({
    this.id,
    this.reused = false,
    this.protocol,
    this.proxy,
  });

  Map<String, dynamic> toJson() => <String, dynamic>{
        if (id != null) 'id': id,
        'reused': reused,
        if (protocol != null) 'protocol': protocol,
        if (proxy != null) 'proxy': proxy,
      };
}

class FlogTimingPhase {
  final String name;
  final int? startUs;
  final int? endUs;
  final String status;
  final String confidence;
  final String? detail;

  const FlogTimingPhase({
    required this.name,
    this.startUs,
    this.endUs,
    this.status = 'complete',
    this.confidence = 'exact',
    this.detail,
  });

  int? get durationUs =>
      startUs == null || endUs == null ? null : endUs! - startUs!;

  Map<String, dynamic> toJson() => <String, dynamic>{
        'name': name,
        if (startUs != null) 'startUs': startUs,
        if (endUs != null) 'endUs': endUs,
        'status': status,
        'confidence': confidence,
        if (detail != null) 'detail': detail,
      };
}

class FlogTimingEvent {
  final String name;
  final int atUs;
  final int? gapUs;
  final int? size;
  final String? detail;

  const FlogTimingEvent({
    required this.name,
    required this.atUs,
    this.gapUs,
    this.size,
    this.detail,
  });

  Map<String, dynamic> toJson() => <String, dynamic>{
        'name': name,
        'atUs': atUs,
        if (gapUs != null) 'gapUs': gapUs,
        if (size != null) 'size': size,
        if (detail != null) 'detail': detail,
      };
}

class FlogTimingTrace {
  final int version;
  final String source;
  final String clock;
  final int startUs;
  final int? endUs;
  final FlogTimingConnection? connection;
  final List<FlogTimingPhase> phases;
  final List<FlogTimingEvent> events;
  final List<String> notes;

  const FlogTimingTrace({
    this.version = 1,
    required this.source,
    this.clock = 'monotonic_us',
    required this.startUs,
    this.endUs,
    this.connection,
    required this.phases,
    required this.events,
    required this.notes,
  });

  int? get durationUs => endUs == null ? null : endUs! - startUs;

  FlogTimingTrace finish(int endUs) => FlogTimingTrace(
        version: version,
        source: source,
        clock: clock,
        startUs: startUs,
        endUs: endUs,
        connection: connection,
        phases: phases,
        events: events,
        notes: notes,
      );

  Map<String, dynamic> toJson() => <String, dynamic>{
        'v': version,
        'source': source,
        'clock': clock,
        'startUs': startUs,
        if (endUs != null) 'endUs': endUs,
        if (connection != null) 'connection': connection!.toJson(),
        'phases': phases.map((p) => p.toJson()).toList(growable: false),
        'events': events.map((e) => e.toJson()).toList(growable: false),
        'notes': notes,
      };
}
```

- [ ] **Step 4: Implement testable monotonic clock**

Create `flog_dart/lib/src/timing/timing_clock.dart`:

```dart
abstract class FlogTimingClock {
  int nowUs();
}

class StopwatchTimingClock implements FlogTimingClock {
  final Stopwatch _stopwatch;

  StopwatchTimingClock() : _stopwatch = Stopwatch()..start();

  @override
  int nowUs() => _stopwatch.elapsedMicroseconds;
}

class ManualTimingClock implements FlogTimingClock {
  int valueUs;

  ManualTimingClock([this.valueUs = 0]);

  @override
  int nowUs() => valueUs;

  void advanceUs(int deltaUs) {
    valueUs += deltaUs;
  }
}
```

- [ ] **Step 5: Run Dart timing model tests**

Run:

```bash
cd flog_dart && flutter test test/timing/timing_trace_test.dart
```

Expected: PASS.

- [ ] **Step 6: Commit Task 4**

```bash
git add flog_dart/lib/src/timing/timing_trace.dart flog_dart/lib/src/timing/timing_clock.dart flog_dart/test/timing/timing_trace_test.dart
git commit -m "feat(dart): add network timing model"
```

---

## Task 5: Dart Timing Stream and HTTP Adapter Wrapper

**Files:**
- Create: `flog_dart/lib/src/timing/timing_stream.dart`
- Create: `flog_dart/lib/src/timing/timing_adapter.dart`
- Modify: `flog_dart/lib/src/flog_dio.dart`
- Test: `flog_dart/test/timing/timing_stream_test.dart`
- Test: `flog_dart/test/timing/timing_adapter_test.dart`

- [ ] **Step 1: Write failing stream tee tests**

Create `flog_dart/test/timing/timing_stream_test.dart`:

```dart
import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:flog_dart/src/timing/timing_clock.dart';
import 'package:flog_dart/src/timing/timing_stream.dart';

void main() {
  test('records first byte, total bytes, gap, and completion', () async {
    final clock = ManualTimingClock();
    final recorder = TimingStreamRecorder(clock: clock);
    final controller = StreamController<Uint8List>();

    final out = recorder.wrap(controller.stream);
    final seen = <Uint8List>[];
    final done = out.listen(seen.add);

    clock.advanceUs(100);
    controller.add(Uint8List.fromList([1, 2]));
    clock.advanceUs(50);
    controller.add(Uint8List.fromList([3]));
    clock.advanceUs(25);
    await controller.close();
    await done.asFuture<void>();

    expect(seen.map((b) => b.length), [2, 1]);
    expect(recorder.events.map((e) => e.name), ['first_byte', 'chunk', 'complete']);
    expect(recorder.events[0].atUs, 100);
    expect(recorder.events[1].gapUs, 50);
    expect(recorder.events[2].size, 3);
  });
}
```

- [ ] **Step 2: Run failing stream tee tests**

Run:

```bash
cd flog_dart && flutter test test/timing/timing_stream_test.dart
```

Expected: FAIL because `timing_stream.dart` does not exist.

- [ ] **Step 3: Implement `TimingStreamRecorder`**

Create `flog_dart/lib/src/timing/timing_stream.dart`:

```dart
import 'dart:async';
import 'dart:typed_data';

import 'timing_clock.dart';
import 'timing_trace.dart';

class TimingStreamRecorder {
  final FlogTimingClock clock;
  final List<FlogTimingEvent> events = <FlogTimingEvent>[];
  int? _lastUs;
  int _totalBytes = 0;
  bool _sawFirstByte = false;

  TimingStreamRecorder({required this.clock});

  Stream<Uint8List> wrap(Stream<Uint8List> input) {
    late StreamController<Uint8List> controller;
    late StreamSubscription<Uint8List> subscription;

    controller = StreamController<Uint8List>(
      onListen: () {
        subscription = input.listen(
          (chunk) {
            final now = clock.nowUs();
            final gap = _lastUs == null ? null : now - _lastUs!;
            _lastUs = now;
            _totalBytes += chunk.length;
            events.add(FlogTimingEvent(
              name: _sawFirstByte ? 'chunk' : 'first_byte',
              atUs: now,
              gapUs: gap,
              size: chunk.length,
            ));
            _sawFirstByte = true;
            controller.add(chunk);
          },
          onError: (Object error, StackTrace st) {
            final now = clock.nowUs();
            events.add(FlogTimingEvent(
              name: 'stream_error',
              atUs: now,
              size: _totalBytes,
              detail: error.toString(),
            ));
            controller.addError(error, st);
          },
          onDone: () {
            final now = clock.nowUs();
            events.add(FlogTimingEvent(
              name: 'complete',
              atUs: now,
              size: _totalBytes,
            ));
            controller.close();
          },
          cancelOnError: false,
        );
      },
      onPause: () => subscription.pause(),
      onResume: () => subscription.resume(),
      onCancel: () => subscription.cancel(),
    );

    return controller.stream;
  }
}
```

- [ ] **Step 4: Write failing adapter wrapper test**

Create `flog_dart/test/timing/timing_adapter_test.dart`:

```dart
import 'dart:async';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flog_dart/src/timing/timing_adapter.dart';
import 'package:flog_dart/src/timing/timing_clock.dart';

class _FakeAdapter implements HttpClientAdapter {
  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    return ResponseBody(
      Stream<Uint8List>.fromIterable([
        Uint8List.fromList([1, 2, 3]),
      ]),
      200,
      headers: {
        Headers.contentTypeHeader: ['text/plain'],
      },
    );
  }

  @override
  void close({bool force = false}) {}
}

void main() {
  test('wrapper stores timing trace in RequestOptions.extra', () async {
    final clock = ManualTimingClock();
    final adapter = FlogTimingHttpClientAdapter.wrap(_FakeAdapter(), clock: clock);
    final options = RequestOptions(path: '/x', baseUrl: 'https://example.com');

    clock.advanceUs(10);
    final body = await adapter.fetch(options, null, null);
    expect(body.statusCode, 200);
    expect(options.extra.containsKey(kFlogTimingTraceExtraKey), isTrue);

    final stream = body.stream.cast<Uint8List>();
    await stream.toList();
    final trace = options.extra[kFlogTimingTraceExtraKey];
    expect(trace.toJson()['source'], 'custom_adapter');
  });
}
```

- [ ] **Step 5: Implement minimal adapter wrapper**

Create `flog_dart/lib/src/timing/timing_adapter.dart`:

```dart
import 'dart:typed_data';

import 'package:dio/dio.dart';

import 'timing_clock.dart';
import 'timing_stream.dart';
import 'timing_trace.dart';

const String kFlogTimingTraceExtraKey = '_flog_timing_trace';

class FlogTimingHttpClientAdapter implements HttpClientAdapter {
  final HttpClientAdapter _inner;
  final FlogTimingClock _clock;
  final String _source;

  FlogTimingHttpClientAdapter.wrap(
    this._inner, {
    FlogTimingClock? clock,
    String source = 'custom_adapter',
  })  : _clock = clock ?? StopwatchTimingClock(),
        _source = source;

  @override
  Future<ResponseBody> fetch(
    RequestOptions options,
    Stream<Uint8List>? requestStream,
    Future<void>? cancelFuture,
  ) async {
    final startUs = _clock.nowUs();
    final response = await _inner.fetch(options, requestStream, cancelFuture);
    final headersUs = _clock.nowUs();
    final recorder = TimingStreamRecorder(clock: _clock);
    final wrapped = recorder.wrap(response.stream.cast<Uint8List>());

    final trace = FlogTimingTrace(
      source: _source,
      startUs: startUs,
      endUs: headersUs,
      phases: [
        FlogTimingPhase(
          name: 'adapter',
          startUs: startUs,
          endUs: headersUs,
          status: 'complete',
          confidence: 'exact',
        ),
      ],
      events: const [],
      notes: _source == 'custom_adapter'
          ? const ['custom adapter did not expose socket timing']
          : const [],
    );
    options.extra[kFlogTimingTraceExtraKey] = trace;

    return ResponseBody(
      wrapped,
      response.statusCode,
      headers: response.headers,
      statusMessage: response.statusMessage,
      isRedirect: response.isRedirect,
      redirects: response.redirects,
      extra: response.extra,
    );
  }

  @override
  void close({bool force = false}) {
    _inner.close(force: force);
  }
}
```

- [ ] **Step 6: Install adapter wrapper in `FlogDio`**

In `flog_dart/lib/src/flog_dio.dart`, import:

```dart
import 'timing/timing_adapter.dart';
```

After `_inner` is created and before interceptors are inserted:

```dart
_inner.httpClientAdapter = FlogTimingHttpClientAdapter.wrap(
  _inner.httpClientAdapter,
  source: 'flog_adapter',
);
```

Place this inside `if (flogEnabled) { ... }` so release tree-shaking keeps disabled builds clean.

- [ ] **Step 7: Run Dart timing adapter tests**

Run:

```bash
cd flog_dart && flutter test test/timing/timing_stream_test.dart test/timing/timing_adapter_test.dart
```

Expected: PASS.

- [ ] **Step 8: Run existing FlogDio tests**

Run:

```bash
cd flog_dart && flutter test test/flog_dio_test.dart
```

Expected: PASS. If construction tests fail because adapter type changed, update assertions to verify interceptor order still holds and the adapter is a `FlogTimingHttpClientAdapter`.

- [ ] **Step 9: Commit Task 5**

```bash
git add flog_dart/lib/src/timing/timing_stream.dart flog_dart/lib/src/timing/timing_adapter.dart flog_dart/lib/src/flog_dio.dart flog_dart/test/timing/timing_stream_test.dart flog_dart/test/timing/timing_adapter_test.dart flog_dart/test/flog_dio_test.dart
git commit -m "feat(dart): wrap dio responses with timing"
```

---

## Task 6: Dart HTTP Interceptor and Mock Timing Emission

**Files:**
- Modify: `flog_dart/lib/src/flog_http_interceptor.dart`
- Modify: `flog_dart/lib/src/flog_mock_interceptor.dart`
- Test: `flog_dart/test/flog_http_interceptor_test.dart`
- Test: `flog_dart/test/flog_mock_interceptor_test.dart`

- [ ] **Step 1: Write failing HTTP emission test**

Add to `flog_dart/test/flog_http_interceptor_test.dart`:

```dart
test('normal response emits timing when RequestOptions.extra contains trace', () {
  final interceptor = FlogHttpInterceptor();
  final opts = _opts('https://example.com/timing');
  interceptor.onRequest(opts, _ReqHandler());
  opts.extra['_flog_timing_trace'] = const FlogTimingTrace(
    source: 'flog_adapter',
    startUs: 0,
    endUs: 126000,
    phases: [
      FlogTimingPhase(name: 'ttfb', startUs: 62000, endUs: 104000),
    ],
    events: [],
    notes: [],
  );

  FlogStore.instance.clear();
  final response = Response<dynamic>(
    requestOptions: opts,
    statusCode: 200,
    data: 'ok',
  );
  interceptor.onResponse(response, _ResHandler());

  final rec = _nets().single;
  expect(rec['timing']['source'], 'flog_adapter');
  expect(rec['timing']['phases'][0]['name'], 'ttfb');
});
```

Add imports:

```dart
import 'package:flog_dart/src/timing/timing_trace.dart';
```

- [ ] **Step 2: Run failing HTTP emission test**

Run:

```bash
cd flog_dart && flutter test test/flog_http_interceptor_test.dart --plain-name "normal response emits timing"
```

Expected: FAIL because `FlogHttpInterceptor` does not emit timing.

- [ ] **Step 3: Emit timing from `_emitHttpCompletion` and `onError`**

In `flog_http_interceptor.dart`, import:

```dart
import 'timing/timing_adapter.dart' show kFlogTimingTraceExtraKey;
import 'timing/timing_trace.dart' show FlogTimingTrace;
```

In `_emitHttpCompletion`, read:

```dart
final timing = response.requestOptions.extra.remove(kFlogTimingTraceExtraKey)
    as FlogTimingTrace?;
if (timing != null) {
  data['timing'] = timing.toJson();
}
```

In `onError`, after building `data` for `res` or `err`, add:

```dart
final timing = err.requestOptions.extra.remove(kFlogTimingTraceExtraKey)
    as FlogTimingTrace?;
if (timing != null) {
  data['timing'] = timing.toJson();
}
```

- [ ] **Step 4: Preserve mocked request timing**

In `flog_mock_interceptor.dart`, add:

```dart
const String kFlogMockDelayMsExtrasKey = 'flog_mock_delay_ms';
```

When a rule matches:

```dart
options.extra[kFlogMockDelayMsExtrasKey] = rule.delayMs;
```

In `flog_http_interceptor.dart`, import that key and build a mocked trace in the mocked path:

```dart
final mockDelay = response.requestOptions.extra[kFlogMockDelayMsExtrasKey] as int? ?? 0;
final timing = FlogTimingTrace(
  source: 'interceptor',
  startUs: 0,
  endUs: mockDelay * 1000,
  phases: [
    FlogTimingPhase(
      name: 'mock_delay',
      startUs: 0,
      endUs: mockDelay * 1000,
      status: 'complete',
      confidence: 'exact',
    ),
  ],
  events: const [],
  notes: mockDelay > 0
      ? const ['mock rule delay applied before response']
      : const ['mock response resolved without network'],
);
response.requestOptions.extra[kFlogTimingTraceExtraKey] = timing;
```

- [ ] **Step 5: Run HTTP and mock tests**

Run:

```bash
cd flog_dart && flutter test test/flog_http_interceptor_test.dart test/flog_mock_interceptor_test.dart
```

Expected: PASS.

- [ ] **Step 6: Commit Task 6**

```bash
git add flog_dart/lib/src/flog_http_interceptor.dart flog_dart/lib/src/flog_mock_interceptor.dart flog_dart/test/flog_http_interceptor_test.dart flog_dart/test/flog_mock_interceptor_test.dart
git commit -m "feat(dart): emit http timing traces"
```

---

## Task 7: Dart SSE Timing

**Files:**
- Modify: `flog_dart/lib/src/sse/reporter.dart`
- Test: `flog_dart/test/sse/reporter_test.dart`

- [ ] **Step 1: Write failing SSE reporter emission test**

Because current reporter tests do not inspect emitted nets directly, add a test that verifies stream behavior and timing events through `FlogStore.snapshotForTesting`:

```dart
test('chunks include eventTiming and done includes full timing', () async {
  FlogStore.instance.clear();
  final events = const [
    SseEvent(data: 'a'),
    SseEvent(data: 'b'),
  ];

  await Stream<SseEvent>.fromIterable(events)
      .transform(const FlogSseReporter(url: 'https://x.com/sse'))
      .toList();

  final nets = FlogStore.instance.snapshotForTesting
      .where((m) => m['type'] == 'net')
      .toList(growable: false);
  final chunks = nets.where((m) => m['t'] == 'chunk').toList();
  final done = nets.singleWhere((m) => m['t'] == 'done');

  expect(chunks, hasLength(2));
  expect(chunks.first['eventTiming']['name'], 'chunk');
  expect(chunks.first['eventTiming']['atUs'], isA<int>());
  expect(done['timing']['source'], 'sse_reporter');
  expect(done['timing']['phases'], isA<List>());
});
```

Add import:

```dart
import 'package:flog_dart/flog_dart.dart' show FlogStore;
```

- [ ] **Step 2: Run failing SSE test**

Run:

```bash
cd flog_dart && flutter test test/sse/reporter_test.dart --plain-name "chunks include eventTiming"
```

Expected: FAIL because chunks/done do not include timing.

- [ ] **Step 3: Add timing trace to `FlogSseReporter`**

In `reporter.dart`, import:

```dart
import '../timing/timing_clock.dart';
import '../timing/timing_trace.dart';
```

Inside `bind`, create:

```dart
final clock = StopwatchTimingClock();
final startUs = clock.nowUs();
int? lastEventUs;
final timingEvents = <FlogTimingEvent>[];
```

When emitting chunk:

```dart
final nowUs = clock.nowUs();
final eventTiming = FlogTimingEvent(
  name: 'chunk',
  atUs: nowUs,
  gapUs: lastEventUs == null ? nowUs - startUs : nowUs - lastEventUs!,
  size: event.data.length,
);
lastEventUs = nowUs;
timingEvents.add(eventTiming);
emit('chunk', {
  'data': event.data,
  'seq': seq,
  'eventTiming': eventTiming.toJson(),
});
```

On done:

```dart
final endUs = clock.nowUs();
final trace = FlogTimingTrace(
  source: 'sse_reporter',
  startUs: startUs,
  endUs: endUs,
  phases: [
    FlogTimingPhase(
      name: 'stream_open',
      startUs: startUs,
      endUs: endUs,
      status: 'complete',
      confidence: 'exact',
    ),
  ],
  events: timingEvents,
  notes: const [],
);
emit('done', {
  'duration': Duration(microseconds: endUs - startUs).inMilliseconds,
  'chunks': seq,
  'timing': trace.toJson(),
});
```

On error, emit `err` with a trace whose phase status is `errored`.

- [ ] **Step 4: Run SSE tests**

Run:

```bash
cd flog_dart && flutter test test/sse/reporter_test.dart
```

Expected: PASS.

- [ ] **Step 5: Commit Task 7**

```bash
git add flog_dart/lib/src/sse/reporter.dart flog_dart/test/sse/reporter_test.dart
git commit -m "feat(dart): add sse timing events"
```

---

## Task 8: Dart WebSocket Timing

**Files:**
- Modify: `flog_dart/lib/src/flog_web_socket.dart`
- Test: `flog_dart/test/flog_web_socket_test.dart`
- Test: `flog_dart/test/flog_web_socket_connect_test.dart`

- [ ] **Step 1: Write failing WS timing tests**

Add to `flog_dart/test/flog_web_socket_test.dart`:

```dart
test('send and recv include eventTiming', () async {
  final channel = _FakeChannel();
  final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
  final sub = ws.stream.listen((_) {});
  FlogStore.instance.clear();

  ws.send('hi');
  channel.push('pong');
  await Future<void>.delayed(const Duration(milliseconds: 10));
  await sub.cancel();

  final sends = _nets().where((r) => r['t'] == 'send').toList();
  final recvs = _nets().where((r) => r['t'] == 'recv').toList();
  expect(sends.single['eventTiming']['name'], 'send');
  expect(recvs.single['eventTiming']['name'], 'recv');
  expect(recvs.single['eventTiming']['atUs'], isA<int>());
});

test('close includes timing trace', () async {
  final channel = _FakeChannel();
  final ws = FlogWebSocket.fromChannel(channel, url: 'wss://x/y');
  final sub = ws.stream.listen((_) {}, onError: (_) {});
  FlogStore.instance.clear();

  await ws.close(1000, 'bye').timeout(const Duration(seconds: 2));
  await sub.cancel();

  final close = _nets().singleWhere((r) => r['t'] == 'close');
  expect(close['timing']['source'], 'ws_wrapper');
  expect(close['timing']['phases'][0]['name'], 'open');
});
```

- [ ] **Step 2: Run failing WS tests**

Run:

```bash
cd flog_dart && flutter test test/flog_web_socket_test.dart --plain-name "include eventTiming"
```

Expected: FAIL because WS frames do not include timing.

- [ ] **Step 3: Add timing fields to `FlogWebSocket`**

In `flog_web_socket.dart`, import:

```dart
import 'timing/timing_clock.dart';
import 'timing/timing_trace.dart';
```

Add fields:

```dart
final FlogTimingClock _clock;
final int _startUs;
int? _lastMessageUs;
final List<FlogTimingEvent> _events = <FlogTimingEvent>[];
```

Initialize these in constructors using `StopwatchTimingClock`. For constructors that receive an existing `DateTime start`, keep `_start` for existing `duration` behavior and add a monotonic `_startUs`.

- [ ] **Step 4: Emit `eventTiming` for send/recv**

Add helper:

```dart
FlogTimingEvent _eventTiming(String name, int size) {
  final nowUs = _clock.nowUs();
  final event = FlogTimingEvent(
    name: name,
    atUs: nowUs,
    gapUs: _lastMessageUs == null ? nowUs - _startUs : nowUs - _lastMessageUs!,
    size: size,
  );
  _lastMessageUs = nowUs;
  _events.add(event);
  return event;
}
```

In `send` and recv mapping, add:

```dart
final eventTiming = _eventTiming('send', size);
...
'eventTiming': eventTiming.toJson(),
```

and:

```dart
final eventTiming = _eventTiming('recv', size);
...
'eventTiming': eventTiming.toJson(),
```

- [ ] **Step 5: Emit full timing on close and connect failure**

Add helper:

```dart
Map<String, dynamic> _traceJson(String terminalPhase, int endUs) {
  return FlogTimingTrace(
    source: 'ws_wrapper',
    startUs: _startUs,
    endUs: endUs,
    phases: [
      FlogTimingPhase(
        name: terminalPhase,
        startUs: _startUs,
        endUs: endUs,
        status: 'complete',
        confidence: 'exact',
      ),
    ],
    events: List.unmodifiable(_events),
    notes: const [],
  ).toJson();
}
```

In `close`, add:

```dart
data['timing'] = _traceJson('open', _clock.nowUs());
```

In `_connectAndWrap` failure path, emit `timing` with phase `handshake` and status `errored`.

- [ ] **Step 6: Run WS tests**

Run:

```bash
cd flog_dart && flutter test test/flog_web_socket_test.dart test/flog_web_socket_connect_test.dart
```

Expected: PASS.

- [ ] **Step 7: Commit Task 8**

```bash
git add flog_dart/lib/src/flog_web_socket.dart flog_dart/test/flog_web_socket_test.dart flog_dart/test/flog_web_socket_connect_test.dart
git commit -m "feat(dart): add websocket timing events"
```

---

## Task 9: Protocol and Module Documentation

**Files:**
- Modify: `docs/PROTOCOL.md`
- Modify: `docs/MODULES.md`
- Modify: `docs/ARCHITECTURE.md` if the high-level data flow needs one short timing mention.
- Modify: `flog_dart/README.md` if public user-facing docs need a brief note.

- [ ] **Step 1: Update protocol timing section**

In `docs/PROTOCOL.md`, add a new subsection under `FlogNetKind`:

```markdown
### 4.x Network timing

New clients may attach `timing` to terminal/state frames (`res`, `err`,
`done`, `open`, `close`) and `eventTiming` to event frames (`chunk`,
`send`, `recv`). Both fields are optional and additive.

`timing.clock` is `monotonic_us`; values are relative to one trace and
must not be compared across entries. Wall-clock `ts` remains the display
timestamp.

Unavailable phases are represented through phase `status` and
`confidence`, not as `0ms` durations.
```

Include the JSON examples from the design spec.

- [ ] **Step 2: Update module docs**

In `docs/MODULES.md`, add:

```markdown
### `src/domain/network_timing.rs`

- **Purpose:** pure DevTools-style timing data types shared by HTTP, SSE,
  and WebSocket Network detail renderers.
- **Key types:** `NetworkTiming`, `TimingPhase`, `TimingEvent`,
  `TimingConnection`, `TimingSource`, `TimingPhaseStatus`,
  `TimingConfidence`.
- **Dependencies:** serde only.
- **Tests:** covered through `network_tests.rs` and
  `network_store_tests.rs`.
```

Add `src/ui/network/detail/timing.rs` under UI modules:

```markdown
### `src/ui/network/detail/timing.rs`

- **Purpose:** renders the Network detail Timing section. HTTP uses a phase
  waterfall, SSE uses stream/event gaps, and WebSocket uses connection and
  message timelines.
- **Dependencies:** ratatui, `domain::network`, `domain::network_timing`.
```

- [ ] **Step 3: Run doc-adjacent verification**

Run:

```bash
rg -n "timing|eventTiming|NetworkTiming" docs/PROTOCOL.md docs/MODULES.md docs/ARCHITECTURE.md flog_dart/README.md
cargo test --all
cd flog_dart && flutter test
```

Expected: `rg` shows the new docs entries; Rust and Dart tests pass.

- [ ] **Step 4: Commit Task 9**

```bash
git add docs/PROTOCOL.md docs/MODULES.md docs/ARCHITECTURE.md flog_dart/README.md
git commit -m "docs: document network timing protocol"
```

---

## Task 10: Final Verification

**Files:**
- No planned code edits.

- [ ] **Step 1: Run Rust formatting and lint**

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
```

Expected: PASS.

- [ ] **Step 2: Run Rust tests**

```bash
cargo test --all
```

Expected: PASS.

- [ ] **Step 3: Run Dart tests and analyzer**

```bash
cd flog_dart && flutter test && dart analyze
```

Expected: PASS.

- [ ] **Step 4: Manual TUI smoke check with fake server**

Run:

```bash
cargo test --test ws_connect_test -- --nocapture
```

Expected: existing WebSocket integration still passes. If a timing-aware fake server is added during implementation, verify that selecting a timed HTTP/SSE/WS entry shows `▼ Timing` only in the detail panel and the Network list columns remain unchanged.

- [ ] **Step 5: Commit final cleanup if verification required mechanical fixes**

If formatting or small doc/test fixes were needed:

```bash
git add .
git commit -m "chore: verify network timing feature"
```

If no files changed, skip this commit.

---

## Self-Review

**Spec coverage**

- Hybrid adapter/default wrapping: Task 5.
- HTTP/SSE/WS timing: Tasks 5, 6, 7, 8.
- Optional additive protocol: Tasks 1, 2, 9.
- Pure domain data: Tasks 1, 2.
- Network list unchanged: Tasks 3 and 10 explicitly preserve it.
- Protocol-specific detail UI: Task 3.
- Degradation states: Tasks 1, 3, 5, 6, 8.
- Tests and docs: Tasks 1-10.

**Placeholder scan**

The placeholder scan is clean. Each task includes concrete file paths, code shape, commands, and expected outcomes.

**Type consistency**

Wire names are camelCase (`startUs`, `eventTiming`) and Rust fields use snake_case through serde rename attributes. Dart `toJson()` emits the same wire names that Rust deserializes.
