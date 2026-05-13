//! Small sub-state structs held by [`super::App`]: search matches,
//! input buffers, stats snapshot, detail-panel state, Logs-tab view
//! state.

use crate::domain::LogLevel;

use super::InputField;

/// Match tracking for n/N navigation (results of the Search input field).
#[derive(Default)]
pub struct SearchState {
    pub matches: Vec<usize>,
    pub match_idx: usize,
}

/// Buffers + cursor for all 5 input fields.
#[derive(Default)]
pub struct InputBuffers {
    pub log_search: String,
    pub log_exclude: String,
    pub log_tag: String,
    pub net_search: String,
    pub net_exclude: String,
    pub log_search_cursor: usize,
    pub log_exclude_cursor: usize,
    pub log_tag_cursor: usize,
    pub net_search_cursor: usize,
    pub net_exclude_cursor: usize,
}

impl InputBuffers {
    pub fn buffer_mut(&mut self, field: InputField) -> &mut String {
        match field {
            InputField::LogSearch => &mut self.log_search,
            InputField::LogExclude => &mut self.log_exclude,
            InputField::LogTag => &mut self.log_tag,
            InputField::NetSearch => &mut self.net_search,
            InputField::NetExclude => &mut self.net_exclude,
        }
    }

    pub fn buffer(&self, field: InputField) -> &str {
        match field {
            InputField::LogSearch => &self.log_search,
            InputField::LogExclude => &self.log_exclude,
            InputField::LogTag => &self.log_tag,
            InputField::NetSearch => &self.net_search,
            InputField::NetExclude => &self.net_exclude,
        }
    }

    pub fn cursor_mut(&mut self, field: InputField) -> &mut usize {
        match field {
            InputField::LogSearch => &mut self.log_search_cursor,
            InputField::LogExclude => &mut self.log_exclude_cursor,
            InputField::LogTag => &mut self.log_tag_cursor,
            InputField::NetSearch => &mut self.net_search_cursor,
            InputField::NetExclude => &mut self.net_exclude_cursor,
        }
    }

    pub fn cursor(&self, field: InputField) -> usize {
        match field {
            InputField::LogSearch => self.log_search_cursor,
            InputField::LogExclude => self.log_exclude_cursor,
            InputField::LogTag => self.log_tag_cursor,
            InputField::NetSearch => self.net_search_cursor,
            InputField::NetExclude => self.net_exclude_cursor,
        }
    }
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
    /// JSON viewer fold/unfold state (AST-based).
    pub viewer_state: crate::ui::json_viewer::JsonViewerState,
    /// Cached JSON tree for the currently shown entry. `None` until the
    /// renderer parses the first body.
    pub viewer_tree: Option<crate::ui::json_viewer::Tree>,
    /// Maps body-content row index -> hot regions for click dispatch. Set by renderer.
    pub viewer_click_map: Vec<Vec<crate::ui::json_viewer::JsonHotRegion>>,
    /// Fingerprint of the JSON text the viewer_state was built against. Used by
    /// the renderer to detect "selected entry changed" for any code path — not
    /// just the keyboard/mouse handlers that explicitly call
    /// `reset_detail_for_selection`.
    pub viewer_text_fingerprint: u64,
    /// Currently highlighted row in the JSON viewer (keyboard cursor).
    /// `None` = no cursor / cursor not yet initialised.
    /// Task 4 wires up the J/K navigation that changes this; Task 3 reads
    /// it for the `o` (open URL) shortcut.
    pub viewer_cursor: Option<usize>,
}

/// State for the full-value overlay (Task 5).
///
/// Shown when the user activates `ExpandFullValue` on a truncated string
/// node in the JSON detail viewer. The overlay displays the raw string
/// with scrolling and allows the user to copy it to the clipboard.
#[derive(Debug, Clone, PartialEq)]
pub struct FullValueOverlayState {
    pub text: String,
    pub node_id: u32,
    pub scroll: usize,
}

/// Logs tab view state.
///
/// Phase 3 Step 3.10 (audit UI-003) — introduced for symmetry with
/// [`super::NetworkState`]. Phase 4 completed UI-003 by making this
/// struct the single source of truth for the Logs tab's viewport; the
/// Logs-side scroll methods on [`super::App`] are thin dispatchers
/// that mutate these fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogsViewState {
    pub selected: usize,
    pub scroll_offset: usize,
    /// Auto-scroll to bottom when new logs arrive.
    pub auto_scroll: bool,
}

impl Default for LogsViewState {
    fn default() -> Self {
        Self {
            selected: 0,
            scroll_offset: 0,
            auto_scroll: true,
        }
    }
}
