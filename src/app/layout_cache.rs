//! UI layout coordinate cache.
//!
//! The renderer writes cell-space geometry here each frame; the event
//! handler reads it back to translate mouse coordinates into logical
//! widgets. See audit UI-017.

use std::time::Instant;

/// UI layout coordinate cache (written by renderer, read by event handler).
///
/// Every field is a snapshot of the last rendered frame's geometry. The
/// renderer overwrites the relevant fields on each draw; the event handler
/// reads them to translate mouse coordinates back into logical widgets
/// (rows, pills, buttons, etc.).
///
/// Invariants
/// - All fields reset cleanly via `Default` — no state is retained across
///   sessions (verified by `app_new_starts_with_default_layout_cache`).
/// - Fields are ONLY read by `src/event.rs` and written by `src/ui/*`.
///   No domain or transport code touches this struct.
/// - Coordinates are terminal cells (not pixels). `(x, y)` origin = top-left.
#[derive(Default)]
pub struct LayoutCache {
    pub toolbar_y: u16,
    /// Y position of op row 2 (tag filter + level buttons). Set by renderer.
    pub toolbar_op2_y: u16,
    /// Y position of the column header row (Logs tab). Set by renderer.
    pub col_header_y: u16,
    /// Y position of the column header row (Network tab). Set by renderer.
    pub net_col_header_y: u16,
    pub list_y: u16,
    pub list_height: u16,
    pub bottom_y: u16,
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
    /// Click region for [Copy] button in Logs detail panel title: (y, x_start, x_end)
    pub detail_copy_btn: Option<(u16, u16, u16)>,
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
    /// Device picker item click regions: (y, x_start, x_end, item_index).
    pub device_picker_items: Vec<(u16, u16, u16, usize)>,
    /// Device picker overlay rect: (x, y, w, h).
    pub device_picker_rect: Option<(u16, u16, u16, u16)>,
    /// Device picker item IDs (parallel to device_picker_items indices).
    pub device_picker_item_ids: Vec<String>,
    /// Total line count in device picker content (for scroll clamping).
    pub device_picker_total_lines: usize,
    /// Y position of the row that holds all input fields (logs op1 / network op1).
    pub input_row_y: u16,
    /// Click hit regions per input field.
    pub log_search_x: (u16, u16),
    pub log_exclude_x: (u16, u16),
    pub log_tag_x: (u16, u16),
    pub net_exclude_x: (u16, u16),
    /// Jump-to-bottom floating overlay rect: (x, y, w, h). None when hidden.
    pub jump_to_bottom_rect: Option<(u16, u16, u16, u16)>,
}
