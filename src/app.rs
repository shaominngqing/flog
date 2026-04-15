//! Application state machine.

use std::collections::BTreeSet;
use std::time::Instant;

use regex::Regex;

use crate::domain::{FilterState, LogEntry, LogLevel, LogStore, ParseResult};
use crate::input::{ClientInfo, ConnectorHandle};
use crate::parser::MultiStrategyParser;

/// Infer tag and level from unrecognized raw text content.
#[allow(dead_code)]
fn infer_system_tag(text: &str) -> (LogLevel, &'static str) {
    static EXCEPTION_RE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"(?i)══.*exception|exception caught|thrown").unwrap()
    });
    static STACKTRACE_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"^#\d+\s+").unwrap());

    let trimmed = text.trim_start();

    // Flutter framework exception output
    if trimmed.starts_with('═')
        || EXCEPTION_RE.is_match(trimmed)
        || trimmed.starts_with("Handler:")
        || trimmed.starts_with("Recognizer:")
        || trimmed.starts_with("The following")
        || trimmed.starts_with("When the exception")
    {
        return (LogLevel::Error, "Flutter");
    }

    // Dart stacktrace
    if STACKTRACE_RE.is_match(trimmed) || trimmed.starts_with("(elided") {
        return (LogLevel::Error, "Stacktrace");
    }

    // Dart/Flutter assertion
    if trimmed.starts_with("Failed assertion") || trimmed.starts_with("'package:") {
        return (LogLevel::Error, "Assert");
    }

    // General stdout — likely print() or debugPrint()
    (LogLevel::System, "stdout")
}

// ── Mode ──

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    Search,
    TagFilter,
    Help,
    Stats,
    MockRuleEdit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTab {
    Logs,
    Network,
}

// ── Sub-state structs ──

/// Search input and match tracking.
#[derive(Default)]
pub struct SearchState {
    pub input: String,
    pub matches: Vec<usize>,
    pub match_idx: usize,
}

/// Tag filter input buffer.
#[derive(Default)]
pub struct TagFilterInput {
    pub input: String,
}

/// Snapshot of statistics data (computed once on Stats entry).
pub struct StatsSnapshot {
    pub level_counts: Vec<(LogLevel, u64)>,
    pub tag_ranking: Vec<(String, usize)>,
    pub total: usize,
    pub filtered: usize,
}

/// Detail view state.
#[derive(Default)]
pub struct DetailState {
    pub scroll: usize,
    /// Number of header lines in the detail panel (set by renderer).
    pub header_lines: usize,
    /// JSON viewer fold/unfold state.
    pub viewer_state: crate::ui::json_viewer::JsonViewerState,
}

/// A segment in a JSON field path.
#[derive(Clone, Debug, PartialEq)]
pub enum SsePathSegment {
    Key(String),
    Index(usize),
}

/// A saved SSE merge rule: which JSON field path to concatenate across chunks.
#[derive(Clone)]
pub struct SseMergeRule {
    /// JSON field path like `["choices", 0, "delta", "content"]`
    pub field_path: Vec<SsePathSegment>,
    /// Human-readable path string like `choices[0].delta.content`
    pub field_display: String,
}

/// Network tab view state.
pub struct NetworkState {
    pub selected: usize,
    pub scroll_offset: usize,
    /// Auto-scroll to bottom when new requests arrive.
    pub auto_scroll: bool,
    pub show_detail: bool,
    pub show_mock_rules_panel: bool,
    pub detail_scroll: usize,
    pub filter: crate::domain::NetworkFilter,
    /// Whether URL search input is active.
    pub search_active: bool,
    /// Current search input text.
    pub search_input: String,
    /// Section names that are collapsed (folded). Sections not in this set are expanded.
    pub collapsed_sections: std::collections::HashSet<String>,
    /// Maps detail panel line index -> section key (for click-to-toggle). Set by renderer.
    pub detail_section_map: Vec<Option<String>>,
    /// JSON viewer states keyed by section (e.g., "req_headers", "res_body", "sse_0").
    pub json_viewer_states:
        std::collections::HashMap<String, crate::ui::json_viewer::JsonViewerState>,
    /// Maps detail panel line index -> (section_key, source_line) for JSON bracket click.
    pub detail_json_click_map: Vec<Option<(String, usize)>>,
    /// SSE merge rules: URL path (no query params) → merge rule.
    pub sse_merge_rules: std::collections::HashMap<String, SseMergeRule>,
    /// Whether the current SSE detail is showing Merged mode (true) or Events mode (false).
    pub sse_merged_mode: bool,
    /// Index of the currently selected field in Merged mode's field list.
    pub sse_merged_field_idx: usize,
    /// Whether WS detail shows Chat view (true, default) or Raw view (false).
    pub ws_chat_mode: bool,
    filtered_indices: Vec<usize>,
    filter_dirty: bool,
}

impl NetworkState {
    /// Scroll viewport up (mouse wheel, PageUp). Moves both offset and selected.
    pub fn move_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        self.selected = self.selected.saturating_sub(n);
    }

    /// Scroll viewport down (mouse wheel, PageDown). Moves both offset and selected.
    pub fn move_down(&mut self, n: usize, count: usize) {
        if count == 0 {
            return;
        }
        self.scroll_offset = (self.scroll_offset + n).min(count.saturating_sub(1));
        self.selected = (self.selected + n).min(count - 1);
    }

    /// Move selection up (k/Up). Viewport follows if needed.
    pub fn select_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.selected = self.selected.saturating_sub(n);
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    /// Move selection down (j/Down). Renderer adjusts viewport.
    pub fn select_down(&mut self, n: usize, count: usize) {
        if count == 0 {
            return;
        }
        self.selected = (self.selected + n).min(count - 1);
    }

    pub fn go_top(&mut self) {
        self.auto_scroll = false;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn go_bottom(&mut self) {
        self.auto_scroll = true;
    }

    pub fn new() -> Self {
        Self {
            selected: 0,
            scroll_offset: 0,
            auto_scroll: true,
            show_detail: false,
            show_mock_rules_panel: false,
            detail_scroll: 0,
            filter: crate::domain::NetworkFilter::new(),
            search_active: false,
            search_input: String::new(),
            collapsed_sections: std::collections::HashSet::new(),
            detail_section_map: Vec::new(),
            json_viewer_states: std::collections::HashMap::new(),
            detail_json_click_map: Vec::new(),
            sse_merge_rules: std::collections::HashMap::new(),
            sse_merged_mode: false,
            sse_merged_field_idx: 0,
            ws_chat_mode: true,
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

/// UI layout coordinate cache (written by renderer, read by event handler).
#[derive(Default)]
pub struct LayoutCache {
    pub toolbar_y: u16,
    pub list_y: u16,
    pub list_height: u16,
    pub bottom_y: u16,
    pub timeline_y: u16,
    pub search_x: (u16, u16),
    pub filter_x: (u16, u16),
    pub levels_x: u16,
    pub bottom_buttons: Vec<(&'static str, u16, u16)>,
    pub width: u16,
    pub last_click: Option<(Instant, u16, u16)>,
    /// Maps each display row (0-based within list area) to a filtered index.
    /// Built during rendering, used by mouse click handler.
    pub row_to_filtered_idx: Vec<usize>,
    /// True if the last render showed the final filtered entry.
    pub rendered_to_end: bool,
    /// X-range of the source info text in the status bar (clickable for reconnect).
    pub source_info_x: (u16, u16),
    /// Number of unique filtered entries that were actually visible in the last render.
    /// Accounts for variable-height entries (wrap, separators, extra_lines).
    pub visible_entry_count: usize,
    /// Clickable region of the Logs tab label: (x_start, x_end).
    pub tab_logs_x: (u16, u16),
    /// Clickable region of the Network tab label: (x_start, x_end).
    pub tab_network_x: (u16, u16),
    /// Y position of the view-tab bar.
    pub tab_bar_y: u16,
    /// X position where network detail panel starts (for mouse hit testing).
    pub net_detail_x: u16,
    /// Y position where network detail content starts (set by detail renderer).
    pub net_detail_content_y: u16,
    /// Click region for [Mock] button in detail panel header: (y, x_start, x_end)
    pub detail_mock_btn: Option<(u16, u16, u16)>,
    /// SSE pill line: (all_lines_index, header_text_width) for computing pill click positions.
    pub sse_pill_line: Option<(usize, usize)>,
    /// WS pill line: (all_lines_index, header_text_width) for computing pill click positions.
    pub ws_pill_line: Option<(usize, usize)>,
    /// Network status bar button regions: (name, x_start, x_end).
    pub net_buttons: Vec<(String, u16, u16)>,
    /// Network toolbar Y position.
    pub net_toolbar_y: u16,
    /// Network toolbar search click region.
    pub net_search_x: (u16, u16),
    /// Network filter pill click regions: (id, x_start, x_end).
    pub net_filter_pills: Vec<(String, u16, u16)>,
    /// Y position of the filter pills line.
    pub net_filter_pills_y: u16,
    /// Clickable regions in the mock rules table: (row_idx, action, y, x_start, x_end).
    pub mock_rule_regions: Vec<(usize, String, u16, u16, u16)>,
    /// Clickable regions in the mock rule editor: (field_name, y, x_start, x_end).
    pub mock_edit_regions: Vec<(String, u16, u16, u16)>,
    /// Body editor rect in mock rule editor: (x, y, w, h).
    pub mock_edit_body_rect: Option<(u16, u16, u16, u16)>,
    /// Clickable slowest rows in stats: (store_idx, y, x_start, x_end).
    pub stats_slowest_regions: Vec<(usize, u16, u16, u16)>,
    /// Device picker item click regions: (y, x_start, x_end, device_index).
    pub device_picker_items: Vec<(u16, u16, u16, usize)>,
    /// Device picker overlay rect: (x, y, w, h).
    pub device_picker_rect: Option<(u16, u16, u16, u16)>,
}

// ── App ──

pub struct App {
    // Core
    pub store: LogStore,
    pub filter: FilterState,
    #[allow(dead_code)]
    parser: MultiStrategyParser,
    pub active_tab: ViewTab,
    pub network_store: crate::domain::NetworkStore,
    pub network: NetworkState,

    // Navigation
    pub mode: AppMode,
    pub should_quit: bool,
    /// When true, mouse capture is disabled so the terminal handles text selection.
    pub select_mode: bool,
    pub selected: usize,
    pub scroll_offset: usize,
    pub show_detail_panel: bool,
    pub detail_panel_pct: u16, // detail panel width percentage (20-60)

    // Sub-states
    pub search: SearchState,
    pub tag_filter: TagFilterInput,
    pub detail: DetailState,
    pub bookmarks: BTreeSet<usize>,

    // Source management (Direct Socket connector)
    pub connector_handle: Option<ConnectorHandle>,
    pub server_port: u16,
    pub clients: Vec<ClientInfo>,
    pub source_name: String,
    pub status_message: Option<(String, u64)>, // (message, expire_tick)
    pub connected: bool,
    /// Discovered devices from device_monitor (updated by main.rs)
    pub discovered_devices: Vec<crate::transport::device_monitor::Device>,
    /// Show device picker dropdown
    pub show_device_picker: bool,
    /// Selected index in device picker
    pub device_picker_selected: usize,
    /// Scroll offset in device picker
    pub device_picker_scroll: usize,
    /// Channel to request connection to a specific device
    pub connect_device_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,

    // Mock rules
    pub mock_rules: crate::domain::mock::MockRuleStore,
    pub mock_rule_selected: usize,
    pub mock_edit_rule_id: Option<usize>,
    pub mock_edit_field: usize,
    pub mock_edit_top_values: Vec<String>,
    pub mock_edit_body: crate::ui::text_editor::TextEditor,

    // UI
    pub layout: LayoutCache,
    pub tick: u64,
    pub stats_snapshot: Option<StatsSnapshot>,
    /// Which tab the Stats overlay was opened from (Logs or Network).
    pub active_stats_tab: ViewTab,

    // Scroll
    pub auto_scroll: bool,
    pub new_logs_since_pause: usize,

    // Internal
    filtered_indices: Vec<usize>,
    filter_dirty: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            store: LogStore::new(),
            filter: FilterState::default(),
            parser: MultiStrategyParser::default_chain(),
            active_tab: ViewTab::Logs,
            network_store: crate::domain::NetworkStore::new(),
            network: NetworkState::new(),
            mode: AppMode::Normal,
            should_quit: false,
            select_mode: false,
            selected: 0,
            scroll_offset: 0,
            show_detail_panel: false,
            detail_panel_pct: 35,
            search: SearchState::default(),
            tag_filter: TagFilterInput::default(),
            detail: DetailState::default(),
            bookmarks: BTreeSet::new(),
            connector_handle: None,
            server_port: 9753,
            clients: Vec::new(),
            source_name: String::new(),
            status_message: None,
            connected: false,
            discovered_devices: Vec::new(),
            show_device_picker: false,
            device_picker_selected: 0,
            device_picker_scroll: 0,
            connect_device_tx: None,
            mock_rules: crate::domain::mock::MockRuleStore::new(),
            mock_rule_selected: 0,
            mock_edit_rule_id: None,
            mock_edit_field: 0,
            mock_edit_top_values: Vec::new(),
            mock_edit_body: crate::ui::text_editor::TextEditor::new(""),
            layout: LayoutCache::default(),
            tick: 0,
            stats_snapshot: None,
            active_stats_tab: ViewTab::Logs,
            auto_scroll: true,
            new_logs_since_pause: 0,
            filtered_indices: Vec::new(),
            filter_dirty: true,
        }
    }

    /// Check if there is at least one connected client.
    pub fn has_connected_client(&self) -> bool {
        !self.clients.is_empty()
    }

    // ── Data Input ──

    #[allow(dead_code)]
    pub fn add_raw_line(&mut self, raw: &str) {
        self.add_raw_line_with_timestamp(raw, "");
    }

    #[allow(dead_code)]
    pub fn add_raw_line_with_timestamp(&mut self, raw: &str, timestamp: &str) {
        match self.parser.parse(raw) {
            ParseResult::NewEntry(mut entry) => {
                if entry.timestamp.is_empty() && !timestamp.is_empty() {
                    entry.timestamp = timestamp.to_string();
                }
                self.add_entry(entry);
            }
            ParseResult::Continuation(content) => {
                self.store.append_continuation(content);
                self.filter_dirty = true;
            }
            ParseResult::Ignored => {
                // Never drop a line — show as SYSTEM with tag inferred from content
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    let (level, tag) = infer_system_tag(trimmed);
                    let mut entry = LogEntry::new(level, tag, trimmed);
                    if !timestamp.is_empty() {
                        entry.timestamp = timestamp.to_string();
                    }
                    self.add_entry(entry);
                }
            }
        }
    }

    pub fn add_entry(&mut self, entry: LogEntry) {
        self.connected = true;

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
            self.scroll_offset = self.scroll_offset.saturating_sub(drained);
            self.selected = self.selected.saturating_sub(drained);
        }
        self.filter_dirty = true;

        // Don't compute scroll positions here — the renderer will handle it.
        // Just track whether we should auto-scroll or count missed entries.
        if !self.auto_scroll {
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
            self.selected = 0;
            self.scroll_offset = 0;
        } else if !self.auto_scroll {
            self.selected = self.selected.min(len - 1);
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

    // ── Navigation ──
    //
    // These methods set scroll *intent*. The renderer is the single authority
    // that resolves the actual viewport position each frame, because only the
    // renderer knows how many terminal rows each entry occupies.

    /// Scroll viewport up by n entries.
    pub fn move_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        self.selected = self.selected.saturating_sub(n);
    }

    /// Scroll viewport down by n entries.
    pub fn move_down(&mut self, n: usize) {
        let len = self.filtered_count();
        if len == 0 {
            return;
        }
        self.scroll_offset = (self.scroll_offset + n).min(len.saturating_sub(1));
        self.selected = (self.selected + n).min(len - 1);
        // The renderer will fine-tune scroll_offset and detect if we hit bottom.
    }

    /// Move selection up (keyboard k/Up), viewport follows if needed.
    pub fn select_up(&mut self, n: usize) {
        self.auto_scroll = false;
        self.selected = self.selected.saturating_sub(n);
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
    }

    /// Move selection down (keyboard j/Down), viewport follows if needed.
    pub fn select_down(&mut self, n: usize) {
        let len = self.filtered_count();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + n).min(len - 1);
        // Viewport adjustment is done by the renderer (it knows the real capacity).
    }

    pub fn go_top(&mut self) {
        self.auto_scroll = false;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn go_bottom(&mut self) {
        self.auto_scroll = true;
        self.new_logs_since_pause = 0;
        // The renderer will snap to bottom on next frame.
    }

    // ── Level ──

    pub fn set_level(&mut self, level: LogLevel) {
        self.filter.min_level = level;
        self.invalidate_filter();
    }

    // ── Search ──

    pub fn enter_search(&mut self) {
        self.mode = AppMode::Search;
        self.search.input = self.filter.search_query.clone();
        self.layout.last_click = None;
    }

    pub fn apply_search(&mut self) {
        self.filter.set_search(&self.search.input);
        self.mode = AppMode::Normal;
        self.invalidate_filter();
        self.search.match_idx = 0;
        self.layout.last_click = None;
    }

    pub fn cancel_search(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
    }

    pub fn next_match(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        if let Some(pos) = self.search.matches.iter().position(|&m| m > self.selected) {
            self.search.match_idx = pos;
        } else {
            self.search.match_idx = 0;
        }
        self.selected = self.search.matches[self.search.match_idx];
        self.auto_scroll = false;
        // Renderer will ensure selected is visible.
    }

    pub fn prev_match(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        if let Some(pos) = self.search.matches.iter().rposition(|&m| m < self.selected) {
            self.search.match_idx = pos;
        } else {
            self.search.match_idx = self.search.matches.len() - 1;
        }
        self.selected = self.search.matches[self.search.match_idx];
        self.auto_scroll = false;
        // Renderer will ensure selected is visible.
    }

    // ── Tag Filter ──

    pub fn enter_tag_filter(&mut self) {
        self.mode = AppMode::TagFilter;
        let tags: Vec<String> = self
            .filter
            .tag_include
            .iter()
            .cloned()
            .chain(self.filter.tag_exclude.iter().map(|t| format!("-{}", t)))
            .collect();
        self.tag_filter.input = tags.join(",");
        self.layout.last_click = None;
    }

    pub fn apply_tag_filter(&mut self) {
        self.filter.parse_tag_filter(&self.tag_filter.input);
        self.mode = AppMode::Normal;
        self.invalidate_filter();
        self.layout.last_click = None;
    }

    pub fn cancel_tag_filter(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
    }

    pub fn clear_all_filters(&mut self) {
        self.filter.clear();
        self.invalidate_filter();
    }

    // ── Detail ──

    pub fn toggle_detail_panel(&mut self) {
        self.show_detail_panel = !self.show_detail_panel;
    }

    pub fn reset_detail_for_selection(&mut self) {
        self.detail.scroll = 0;
        self.detail.viewer_state = crate::ui::json_viewer::JsonViewerState::default();
    }

    pub fn toggle_detail_fold(&mut self, source_line: usize) {
        crate::ui::json_viewer::toggle_fold(&mut self.detail.viewer_state, source_line);
    }

    pub fn detail_scroll_up(&mut self, n: usize) {
        self.detail.scroll = self.detail.scroll.saturating_sub(n);
    }

    pub fn detail_scroll_down(&mut self, n: usize) {
        self.detail.scroll += n;
    }

    pub fn selected_store_index(&mut self) -> Option<usize> {
        if self.filter_dirty {
            self.rebuild_filter();
        }
        self.filtered_indices.get(self.selected).copied()
    }

    // ── Stats ──

    fn compute_stats(&mut self) -> StatsSnapshot {
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

    // ── Bookmarks ──

    pub fn toggle_bookmark(&mut self) {
        if let Some(idx) = self.selected_store_index() {
            if !self.bookmarks.remove(&idx) {
                self.bookmarks.insert(idx);
            }
        }
    }

    pub fn is_bookmarked(&self, store_idx: usize) -> bool {
        self.bookmarks.contains(&store_idx)
    }

    // ── Tab switching ──

    pub fn switch_tab(&mut self, tab: ViewTab) {
        self.active_tab = tab;
    }

    // ── Mode switches ──

    pub fn enter_help(&mut self) {
        self.mode = AppMode::Help;
        self.layout.last_click = None;
    }
    pub fn exit_help(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
    }
    pub fn enter_stats(&mut self) {
        self.mode = AppMode::Stats;
        self.active_stats_tab = ViewTab::Logs;
        self.layout.last_click = None;
        // Snapshot stats on entry to prevent flickering from live data changes
        self.stats_snapshot = Some(self.compute_stats());
    }
    pub fn enter_network_stats(&mut self) {
        self.mode = AppMode::Stats;
        self.active_stats_tab = ViewTab::Network;
        self.layout.last_click = None;
    }
    pub fn exit_stats(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
        self.stats_snapshot = None;
    }

    pub fn enter_mock_rules(&mut self) {
        if !self.has_connected_client() {
            self.show_status("Mock unavailable — no client connected".to_string());
            return;
        }
        // Toggle mock rules panel in the right side (like detail panel)
        self.network.show_mock_rules_panel = !self.network.show_mock_rules_panel;
        if self.network.show_mock_rules_panel {
            self.network.show_detail = false; // hide detail when showing rules
        }
    }

    pub fn enter_mock_edit(&mut self, rule_id: usize) {
        if let Some(rule) = self.mock_rules.rules().iter().find(|r| r.id == rule_id) {
            self.mock_edit_rule_id = Some(rule_id);
            self.mock_edit_field = 0;
            self.mock_edit_top_values = vec![
                rule.url_pattern.clone(),
                rule.method.clone().unwrap_or_else(|| "*".to_string()),
                rule.status_code.to_string(),
                rule.delay_ms.to_string(),
            ];
            // Pretty-print JSON body for readability
            let pretty_body = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&rule.response_body) {
                serde_json::to_string_pretty(&val).unwrap_or_else(|_| rule.response_body.clone())
            } else {
                rule.response_body.clone()
            };
            self.mock_edit_body = crate::ui::text_editor::TextEditor::new(&pretty_body);
            self.mode = AppMode::MockRuleEdit;
        }
    }

    pub fn save_mock_edit(&mut self) {
        if let Some(id) = self.mock_edit_rule_id {
            if let Some(rule) = self.mock_rules.get_mut(id) {
                rule.url_pattern = self.mock_edit_top_values[0].clone();
                rule.method = if self.mock_edit_top_values[1] == "*" {
                    None
                } else {
                    Some(self.mock_edit_top_values[1].clone())
                };
                rule.status_code = self.mock_edit_top_values[2].parse().unwrap_or(200);
                rule.delay_ms = self.mock_edit_top_values[3].parse().unwrap_or(0);
                rule.response_body = self.mock_edit_body.content();
            }
        }
        self.mock_edit_rule_id = None;
        self.mode = AppMode::Normal;
    }

    pub fn cancel_mock_edit(&mut self) {
        self.mock_edit_rule_id = None;
        self.mode = AppMode::Normal;
    }

    /// Clear all session data when a client disconnects.
    /// Ensures data from one device doesn't leak into another.
    pub fn clear_session_data(&mut self) {
        self.store = LogStore::new();
        self.network_store = crate::domain::NetworkStore::new();
        self.network = NetworkState::new();
        self.mock_rules = crate::domain::mock::MockRuleStore::new();
        self.mock_rule_selected = 0;
        self.mock_edit_rule_id = None;
        self.mock_edit_field = 0;
        self.mock_edit_top_values = Vec::new();
        self.mock_edit_body = crate::ui::text_editor::TextEditor::new("");
        self.bookmarks.clear();
        self.filter_dirty = true;
        self.scroll_offset = 0;
        self.selected = 0;
        self.auto_scroll = true;
        self.new_logs_since_pause = 0;
    }

    // ── Clear & Separator ──

    /// Clear all logs.
    pub fn clear_logs(&mut self) {
        self.store = LogStore::new();
        self.bookmarks.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.auto_scroll = true;
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
