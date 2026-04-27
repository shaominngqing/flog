//! Input-field control surface: enter/exit/apply for the 5 unified
//! input fields, plus next/prev search-match navigation and
//! clear-all-filters. See audit UI-002.

use super::{App, AppMode, InputField};

impl App {
    pub fn enter_input_field(&mut self, field: InputField) {
        // UI-002: ensure the active tab matches the field's owning tab, so the
        // caller doesn't need to remember to switch tabs before entering an
        // input mode. `field.tab()` is the single source of truth.
        self.active_tab = field.tab();
        // Seed buffer from current filter state if buffer is empty.
        match field {
            InputField::LogSearch => {
                if self.inputs.log_search.is_empty() {
                    self.inputs.log_search = self.filter.search_query.clone();
                    self.inputs.log_search_cursor = self.inputs.log_search.len();
                }
            }
            InputField::LogExclude => {
                if self.inputs.log_exclude.is_empty() {
                    self.inputs.log_exclude = self.filter.exclude_query.clone();
                    self.inputs.log_exclude_cursor = self.inputs.log_exclude.len();
                }
            }
            InputField::LogTag => {
                if self.inputs.log_tag.is_empty() {
                    let tags: Vec<String> = self
                        .filter
                        .tag_include
                        .iter()
                        .cloned()
                        .chain(self.filter.tag_exclude.iter().map(|t| format!("-{}", t)))
                        .collect();
                    self.inputs.log_tag = tags.join("|");
                    self.inputs.log_tag_cursor = self.inputs.log_tag.len();
                }
            }
            InputField::NetSearch => {
                if self.inputs.net_search.is_empty() {
                    self.inputs.net_search = self.network.filter.search.clone();
                    self.inputs.net_search_cursor = self.inputs.net_search.len();
                }
            }
            InputField::NetExclude => {
                if self.inputs.net_exclude.is_empty() {
                    self.inputs.net_exclude = self.network.filter.exclude.clone();
                    self.inputs.net_exclude_cursor = self.inputs.net_exclude.len();
                }
            }
        }
        self.mode = AppMode::InputActive(field);
        self.layout.last_click = None;
    }

    pub fn exit_input_field(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
    }

    /// Push the active buffer into the filter and re-run filter.
    pub fn apply_input_field(&mut self, field: InputField) {
        match field {
            InputField::LogSearch => {
                self.filter.set_search(&self.inputs.log_search);
                self.invalidate_filter();
            }
            InputField::LogExclude => {
                self.filter.set_exclude(&self.inputs.log_exclude);
                self.invalidate_filter();
            }
            InputField::LogTag => {
                self.filter.parse_tag_filter(&self.inputs.log_tag);
                self.invalidate_filter();
            }
            InputField::NetSearch => {
                self.network.filter.set_search(&self.inputs.net_search);
                self.network.invalidate_filter();
            }
            InputField::NetExclude => {
                self.network.filter.set_exclude(&self.inputs.net_exclude);
                self.network.invalidate_filter();
            }
        }
    }

    pub fn next_match(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        if let Some(pos) = self
            .search
            .matches
            .iter()
            .position(|&m| m > self.logs.selected)
        {
            self.search.match_idx = pos;
        } else {
            self.search.match_idx = 0;
        }
        self.logs.selected = self.search.matches[self.search.match_idx];
        self.logs.auto_scroll = false;
        // Renderer will ensure selected is visible.
    }

    pub fn prev_match(&mut self) {
        if self.search.matches.is_empty() {
            return;
        }
        if let Some(pos) = self
            .search
            .matches
            .iter()
            .rposition(|&m| m < self.logs.selected)
        {
            self.search.match_idx = pos;
        } else {
            self.search.match_idx = self.search.matches.len() - 1;
        }
        self.logs.selected = self.search.matches[self.search.match_idx];
        self.logs.auto_scroll = false;
        // Renderer will ensure selected is visible.
    }

    pub fn clear_all_filters(&mut self) {
        self.filter.clear();
        // Keep InputBuffers in sync so re-entering a field shows an empty input.
        self.inputs.log_search.clear();
        self.inputs.log_search_cursor = 0;
        self.inputs.log_exclude.clear();
        self.inputs.log_exclude_cursor = 0;
        self.inputs.log_tag.clear();
        self.inputs.log_tag_cursor = 0;
        self.invalidate_filter();
    }
}
