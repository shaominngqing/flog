//! Network tab view state.
//!
//! Mirrors [`super::LogsViewState`] on the Logs side. Owns its own
//! `filtered_indices` write-through cache (audit UI-004) — see
//! [`NetworkState::filtered_indices`] for the cache invariant contract.

use super::SseMergeRule;

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
    /// Section names that are collapsed (folded). Sections not in this set are expanded.
    pub collapsed_sections: std::collections::HashSet<String>,
    /// Maps detail panel line index -> section key (for click-to-toggle). Set by renderer.
    pub detail_section_map: Vec<Option<String>>,
    /// JSON viewer states keyed by section (e.g., "req_headers", "res_body", "sse_0").
    pub json_viewer_states:
        std::collections::HashMap<String, crate::ui::json_viewer::JsonViewerState>,
    /// Maps detail panel line index -> (section_key, node_id) for JSON fold click.
    pub detail_json_click_map: Vec<Option<(String, u32)>>,
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
        self.json_viewer_states.clear();
    }

    /// Move selection down (j/Down). Renderer adjusts viewport.
    pub fn select_down(&mut self, n: usize, count: usize) {
        if count == 0 {
            return;
        }
        self.selected = (self.selected + n).min(count - 1);
        self.json_viewer_states.clear();
    }

    pub fn go_top(&mut self) {
        self.auto_scroll = false;
        self.selected = 0;
        self.scroll_offset = 0;
        self.json_viewer_states.clear();
    }

    pub fn go_bottom(&mut self) {
        self.auto_scroll = true;
        self.json_viewer_states.clear();
    }

    /// Set WS Chat/Raw mode and purge stale collapse keys + viewer states
    /// that belonged to the OTHER mode.
    ///
    /// ## UI-042 (Phase 3 Step 3.8)
    ///
    /// Chat mode records expanded groups with keys `WS_GROUP#<n>`; Raw
    /// mode records collapsed messages with keys `WS#<n>`. Leaving the
    /// opposite mode's keys in `collapsed_sections` corrupts the next
    /// render (an old `WS_GROUP#0` from a previous Chat session reads as
    /// "group 0 expanded" the instant the user flips back to Chat on a
    /// different entry). Additionally, `json_viewer_states` entries keyed
    /// on `ws_*` ids point at AST node IDs for a specific message at a
    /// specific index; toggling modes can change which messages are
    /// rendered, so we drop those states too (they rebuild on next
    /// render with fresh node IDs).
    pub fn set_ws_chat_mode(&mut self, chat: bool) {
        // No-op if mode is unchanged — avoid stomping on genuine chat
        // state when the caller re-asserts the current mode.
        if self.ws_chat_mode == chat {
            return;
        }
        self.ws_chat_mode = chat;
        // Purge the OLD mode's collapse keys. When switching to Chat
        // (chat=true) we drop Raw's WS#* keys; switching to Raw drops
        // Chat's WS_GROUP#* keys.
        let stale_prefix = if chat { "WS#" } else { "WS_GROUP#" };
        self.collapsed_sections
            .retain(|k| !k.starts_with(stale_prefix));
        // Drop all ws_* viewer states; they reference a specific message
        // index + AST node id that may no longer be valid after the
        // toggle. They'll be rebuilt lazily on next render.
        self.json_viewer_states.retain(|k, _| !k.starts_with("ws_"));
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

    /// Marks the `filtered_indices` cache dirty so the next read rebuilds it.
    ///
    /// Call after any mutation that could change which store entries match
    /// the active filter — new entry appended, filter parameters edited,
    /// etc. Cheap; does not do any work itself.
    pub fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    /// Returns the sorted list of store indices that match the current
    /// [`crate::domain::NetworkFilter`], rebuilding the internal cache
    /// lazily on demand.
    ///
    /// ## Cache invariant (audit UI-004)
    ///
    /// `filtered_indices` + `filter_dirty` form a write-through cache:
    ///
    /// - **Rebuild trigger**: `filter_dirty == true`. Set by
    ///   [`Self::invalidate_filter`], which is called by every mutation
    ///   path (`move_up`, `select_down`, `go_top/bottom`, toolbar filter
    ///   edits, every new `NetworkEntry` delivered into `NetworkStore`).
    /// - **Read**: this method reads `filter_dirty`; if clear, returns the
    ///   cached slice unchanged. If dirty, walks the entire store to
    ///   rebuild, then clears the flag.
    /// - **Post-condition**: after returning, `filter_dirty == false` and
    ///   `filtered_indices` is a monotonically-increasing subset of
    ///   `0..store.len()`.
    ///
    /// Callers that merely observe (e.g. the renderer) MUST NOT mutate
    /// `filter_dirty` directly; use `invalidate_filter()`.
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

    /// Convenience wrapper: length of [`Self::filtered_indices`].
    pub fn filtered_count(&mut self, store: &crate::domain::NetworkStore) -> usize {
        self.filtered_indices(store).len()
    }
}

impl Default for NetworkState {
    fn default() -> Self {
        Self::new()
    }
}
