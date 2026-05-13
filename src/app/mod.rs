//! Application state machine.

mod detail;
mod input_fields;
mod layout_cache;
mod mock_edit;
mod mode;
mod multi_app;
mod network_state;
mod scroll;
mod sse_merge;
mod state_structs;

pub use layout_cache::LayoutCache;
pub use mock_edit::MockEditState;
pub use multi_app::ConnectedApp;
pub use network_state::NetworkState;
pub use sse_merge::{SseMergeRule, SsePathSegment};
pub use state_structs::{
    DetailState, FullValueOverlayState, InputBuffers, LogsViewState, SearchState, StatsSnapshot,
};

use std::collections::BTreeSet;
use std::collections::HashMap;

use crate::domain::{FilterState, LogEntry, LogLevel, LogStore};

// ── Mode ──

/// Input field identity for the unified-input-field model.
///
/// Mixes Logs-tab and Network-tab fields in one flat enum (audit UI-002).
/// Rather than split into per-tab enums (which would ripple into every
/// `AppMode::InputActive` match in `src/event.rs`), we expose a `tab()`
/// method that yields the owning tab. Callers that need tab safety can
/// assert `field.tab() == current_tab` before acting — see how the
/// logs/network view modules already structure their dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    LogSearch,
    LogExclude,
    LogTag,
    NetSearch,
    NetExclude,
}

impl InputField {
    /// Returns the `ViewTab` that owns this input field.
    ///
    /// Use when an event handler needs to route based on the active
    /// field's tab, e.g. `if field.tab() == app.active_tab { ... }`.
    pub fn tab(&self) -> ViewTab {
        match self {
            InputField::LogSearch | InputField::LogExclude | InputField::LogTag => ViewTab::Logs,
            InputField::NetSearch | InputField::NetExclude => ViewTab::Network,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    InputActive(InputField),
    Help,
    Stats,
    MockRuleEdit,
    /// Full-value overlay — expands a truncated string node in the JSON
    /// detail viewer into a scrollable overlay. Task 5.
    FullValueOverlay(FullValueOverlayState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    Logs,
    Network,
}

// ── App ──

/// Application state machine — the single mutable root owned by `main`.
///
/// ## Multi-app connection state invariants (audit UI-040 + UI-023 ack)
///
/// flog can be attached to multiple running Flutter apps simultaneously
/// (e.g. one app per device), but the UI shows exactly one at a time.
/// Three collections cooperate to track this state:
///
/// - `connected_apps: Vec<ConnectedApp>` — every app whose WS server has
///   completed the Hello handshake. Entries survive until
///   [`Self::remove_connected_app`] is called (on disconnect).
/// - `active_app_id: Option<String>` — the id currently being viewed, or
///   `None` if nothing is attached.
/// - `discovered_devices: HashMap<String, Device>` — raw device inventory
///   from `flutter devices --machine`, keyed by device id.
///
/// **Invariants (enforced by `add_connected_app` / `remove_connected_app`
/// / `switch_to_app`):**
///
/// 1. `active_app_id == Some(id)` ⇒ `connected_apps` contains an entry
///    whose `.id == id`. Never `active_app_id = Some(x)` without `x` in
///    `connected_apps`.
/// 2. `active_app_id == None` ⇒ `connected_apps.is_empty()` OR we're in
///    a transient remove-and-reassign window (same method call).
/// 3. `discovered_devices` may contain device ids that have NO
///    corresponding entry in `connected_apps` (device visible but not
///    attached yet, or attached then disconnected).
/// 4. `connected_apps` may contain entries whose `device_id` is NOT in
///    `discovered_devices` (device went offline mid-session; we keep the
///    attachment until explicit removal).
/// 5. `device_picker_selected` / `device_picker_scroll` are only
///    meaningful when `show_device_picker == true`; both are clamped by
///    the renderer against `layout.device_picker_items.len()` each frame.
///
/// **Switch semantics:** `switch_to_app(id)` is a no-op unless id is in
/// `connected_apps`. On success it resets session data and sends a
/// `subscribe` on the target app's `ConnectorHandle` so the Dart side
/// replays its buffer.
///
/// **Remove semantics:** `remove_connected_app(id)` retains the entry
/// unconditionally; if it was the active id, it promotes the first
/// remaining entry to active or falls back to `None` with a "Scanning..."
/// source name.
pub struct App {
    // Active session data (points to the currently viewed app's data)
    pub store: LogStore,
    pub filter: FilterState,
    pub active_tab: ViewTab,
    pub network_store: crate::domain::NetworkStore,
    pub network: NetworkState,

    // Navigation
    pub mode: AppMode,
    pub should_quit: bool,
    pub select_mode: bool,
    /// Logs tab viewport state (selected row, scroll offset, auto-scroll flag).
    /// Mirrors `network: NetworkState` on the Network side (audit UI-003).
    pub logs: LogsViewState,
    pub show_detail_panel: bool,
    pub detail_panel_pct: u16,

    // Sub-states
    pub search: SearchState,
    pub inputs: InputBuffers,
    pub detail: DetailState,
    pub bookmarks: BTreeSet<usize>,

    // Multi-app connection management
    /// All connected apps' info.
    pub connected_apps: Vec<ConnectedApp>,
    /// ID of the currently active (viewed) app.
    pub active_app_id: Option<String>,
    /// Server port for display.
    pub server_port: u16,
    pub source_name: String,
    pub status_message: Option<(String, u64)>,
    /// Discovered devices from device_monitor, keyed by device ID.
    pub discovered_devices: HashMap<String, crate::transport::device_monitor::Device>,
    /// Show device picker dropdown.
    pub show_device_picker: bool,
    pub device_picker_selected: usize,
    pub device_picker_scroll: usize,
    /// Channel to request connection to a specific device.
    pub connect_device_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,

    // Mock rules
    pub mock_rules: crate::domain::mock::MockRuleStore,
    pub mock_rule_selected: usize,
    /// Bundled mock rule editor state (audit UI-026 / UI-034).
    pub mock_edit: MockEditState,

    // UI
    pub layout: LayoutCache,
    pub tick: u64,
    pub stats_snapshot: Option<StatsSnapshot>,
    /// Which tab the Stats overlay was opened from (Logs or Network).
    pub active_stats_tab: ViewTab,

    // Scroll
    pub new_logs_since_pause: usize,

    // Internal
    //
    // Logs-tab filter cache (audit UI-018 ack). Mirrors the NetworkState
    // pattern documented on `NetworkState::filtered_indices`. Rebuild is
    // triggered exclusively by `filter_dirty == true`; set via
    // `invalidate_filter()` after any store/filter mutation. Phase 2.5B
    // characterization tests confirm the invariant holds on every mutation
    // path; ack-only entry in this step.
    filtered_indices: Vec<usize>,
    filter_dirty: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            store: LogStore::new(),
            filter: FilterState::default(),
            active_tab: ViewTab::Logs,
            network_store: crate::domain::NetworkStore::new(),
            network: NetworkState::new(),
            mode: AppMode::Normal,
            should_quit: false,
            select_mode: false,
            logs: LogsViewState::default(),
            show_detail_panel: false,
            detail_panel_pct: 35,
            search: SearchState::default(),
            inputs: InputBuffers::default(),
            detail: DetailState::default(),
            bookmarks: BTreeSet::new(),
            connected_apps: Vec::new(),
            active_app_id: None,
            server_port: 9753,
            source_name: String::new(),
            status_message: None,
            discovered_devices: HashMap::new(),
            show_device_picker: false,
            device_picker_selected: 0,
            device_picker_scroll: 0,
            connect_device_tx: None,
            mock_rules: crate::domain::mock::MockRuleStore::new(),
            mock_rule_selected: 0,
            mock_edit: MockEditState::new_blank(),
            layout: LayoutCache::default(),
            tick: 0,
            stats_snapshot: None,
            active_stats_tab: ViewTab::Logs,
            new_logs_since_pause: 0,
            filtered_indices: Vec::new(),
            filter_dirty: true,
        }
    }

    /// Check if there is at least one connected client.
    pub fn has_connected_client(&self) -> bool {
        !self.connected_apps.is_empty()
    }

    // ── Data Input ──

    pub fn add_entry(&mut self, entry: LogEntry) {
        if entry.tag == "flog_net" {
            if let Some(msg) = crate::parser::network::try_parse_network(&entry.tag, &entry.message)
            {
                self.network_store.process_message(msg);
                self.network.invalidate_filter();
                return;
            }
        }

        let drained = self.store.add_entry(entry);
        if drained > 0 {
            let new: BTreeSet<usize> = self
                .bookmarks
                .iter()
                .filter_map(|&idx| idx.checked_sub(drained))
                .collect();
            self.bookmarks = new;
            // Index-space shift, not viewport computation — indices moved, so offset must follow.
            self.logs.scroll_offset = self.logs.scroll_offset.saturating_sub(drained);
            self.logs.selected = self.logs.selected.saturating_sub(drained);
        }
        self.filter_dirty = true;

        // Don't compute scroll positions here — the renderer will handle it.
        // Just track whether we should auto-scroll or count missed entries.
        if !self.logs.auto_scroll {
            self.new_logs_since_pause += 1;
        }
    }

    // ── Filter ──

    pub fn filtered_indices(&mut self) -> &[usize] {
        if self.filter_dirty {
            self.rebuild_filter();
        }
        &self.filtered_indices
    }

    pub fn filtered_count(&mut self) -> usize {
        if self.filter_dirty {
            self.rebuild_filter();
        }
        self.filtered_indices.len()
    }

    fn rebuild_filter(&mut self) {
        self.filtered_indices.clear();
        for (i, entry) in self.store.iter().enumerate() {
            if self.filter.matches(entry) {
                self.filtered_indices.push(i);
            }
        }
        self.filter_dirty = false;

        // Clamp selected to valid range (scroll_offset is clamped by the renderer)
        let len = self.filtered_indices.len();
        if len == 0 {
            self.logs.selected = 0;
            self.logs.scroll_offset = 0;
        } else if !self.logs.auto_scroll {
            self.logs.selected = self.logs.selected.min(len - 1);
        }
        // When auto_scroll is true, the renderer will set selected & scroll_offset.

        // Rebuild search matches
        self.search.matches.clear();
        if !self.filter.search_query.is_empty() {
            for (fi, &store_idx) in self.filtered_indices.iter().enumerate() {
                if let Some(entry) = self.store.get(store_idx) {
                    if !self
                        .filter
                        .search_positions(&entry.full_message())
                        .is_empty()
                        || !self.filter.search_positions(&entry.tag).is_empty()
                    {
                        self.search.matches.push(fi);
                    }
                }
            }
        }
        if self.search.matches.is_empty() {
            self.search.match_idx = 0;
        } else {
            self.search.match_idx = self.search.match_idx.min(self.search.matches.len() - 1);
        }
    }

    pub fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    pub fn selected_store_index(&mut self) -> Option<usize> {
        if self.filter_dirty {
            self.rebuild_filter();
        }
        self.filtered_indices.get(self.logs.selected).copied()
    }

    // ── Stats ──

    pub(super) fn compute_stats(&mut self) -> StatsSnapshot {
        use std::collections::HashMap;
        let mut level_map: HashMap<LogLevel, u64> = HashMap::new();
        let mut tag_map: HashMap<String, usize> = HashMap::new();
        for entry in self.store.iter() {
            *level_map.entry(entry.level).or_insert(0) += 1;
            *tag_map.entry(entry.tag.clone()).or_insert(0) += 1;
        }
        let level_counts = vec![
            (
                LogLevel::Debug,
                level_map.get(&LogLevel::Debug).copied().unwrap_or(0),
            ),
            (
                LogLevel::Info,
                level_map.get(&LogLevel::Info).copied().unwrap_or(0),
            ),
            (
                LogLevel::Warning,
                level_map.get(&LogLevel::Warning).copied().unwrap_or(0),
            ),
            (
                LogLevel::Error,
                level_map.get(&LogLevel::Error).copied().unwrap_or(0),
            ),
            (
                LogLevel::System,
                level_map.get(&LogLevel::System).copied().unwrap_or(0),
            ),
        ];
        let mut tag_ranking: Vec<(String, usize)> = tag_map.into_iter().collect();
        tag_ranking.sort_by(|a, b| b.1.cmp(&a.1));
        StatsSnapshot {
            level_counts,
            tag_ranking,
            total: self.store.len(),
            filtered: self.filtered_count(),
        }
    }

    // ── Clear & Separator ──

    /// Clear all logs.
    pub fn clear_logs(&mut self) {
        self.store = LogStore::new();
        self.bookmarks.clear();
        self.logs.selected = 0;
        self.logs.scroll_offset = 0;
        self.logs.auto_scroll = true;
        self.new_logs_since_pause = 0;
        self.filter_dirty = true;
        self.show_status("Cleared".to_string());
    }

    /// Insert a visual separator line into the log stream.
    pub fn insert_separator(&mut self) {
        let now = timestamp_for_filename();
        let entry = LogEntry {
            timestamp: String::new(),
            level: LogLevel::System,
            tag: "────".to_string(),
            message: format!("──────────────────────── {} ────────────────────────", now),
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: crate::domain::InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        };
        self.add_entry(entry);
        self.show_status("Separator added".to_string());
    }

    // ── Export ──

    pub fn export_logs(&mut self) {
        use std::io::Write;
        if self.filter_dirty {
            self.rebuild_filter();
        }

        let now = timestamp_for_filename();
        let filename = format!("flog_{}.log", now);

        let result = (|| -> std::io::Result<usize> {
            let mut file = std::fs::File::create(&filename)?;
            let mut count = 0;
            for &idx in &self.filtered_indices {
                if let Some(entry) = self.store.get(idx) {
                    writeln!(
                        file,
                        "{} | {:7} | {:14} | {}",
                        entry.timestamp,
                        entry.level.as_str(),
                        entry.tag,
                        entry.full_message()
                    )?;
                    count += 1;
                }
            }
            Ok(count)
        })();

        self.show_status(match result {
            Ok(n) => format!("Exported {} logs to {}", n, filename),
            Err(e) => format!("Export failed: {}", e),
        });
    }

    /// Show a status message for ~2 seconds (60 ticks at 30fps).
    pub fn show_status(&mut self, msg: String) {
        self.status_message = Some((msg, self.tick + 60));
    }

    /// Check if status message has expired.
    pub fn active_status(&self) -> Option<&str> {
        if let Some((ref msg, expire)) = self.status_message {
            if self.tick < expire {
                return Some(msg);
            }
        }
        None
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../app_tests.rs"]
mod tests;

fn timestamp_for_filename() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!(
        "{:02}{:02}{:02}",
        (secs % 86400) / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}
