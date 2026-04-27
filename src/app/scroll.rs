//! Logs-tab navigation: viewport scroll (mouse wheel / PageUp/Down)
//! versus cursor move (j/k), plus go_top/bottom and the level-filter
//! shortcut.
//!
//! These methods set scroll *intent*. The renderer is the single
//! authority that resolves the actual viewport position each frame,
//! because only the renderer knows how many terminal rows each entry
//! occupies.

use crate::domain::LogLevel;

use super::App;

impl App {
    /// Scroll viewport up by n entries.
    pub fn move_up(&mut self, n: usize) {
        self.logs.auto_scroll = false;
        self.logs.scroll_offset = self.logs.scroll_offset.saturating_sub(n);
        self.logs.selected = self.logs.selected.saturating_sub(n);
    }

    /// Scroll viewport down by n entries.
    pub fn move_down(&mut self, n: usize) {
        let len = self.filtered_count();
        if len == 0 {
            return;
        }
        self.logs.scroll_offset = (self.logs.scroll_offset + n).min(len.saturating_sub(1));
        self.logs.selected = (self.logs.selected + n).min(len - 1);
        // The renderer will fine-tune scroll_offset and detect if we hit bottom.
    }

    /// Move selection up (keyboard k/Up), viewport follows if needed.
    pub fn select_up(&mut self, n: usize) {
        self.logs.auto_scroll = false;
        self.logs.selected = self.logs.selected.saturating_sub(n);
        if self.logs.selected < self.logs.scroll_offset {
            self.logs.scroll_offset = self.logs.selected;
        }
        self.reset_detail_for_selection();
    }

    /// Move selection down (keyboard j/Down), viewport follows if needed.
    pub fn select_down(&mut self, n: usize) {
        let len = self.filtered_count();
        if len == 0 {
            return;
        }
        self.logs.selected = (self.logs.selected + n).min(len - 1);
        // Viewport adjustment is done by the renderer (it knows the real capacity).
        self.reset_detail_for_selection();
    }

    pub fn go_top(&mut self) {
        self.logs.auto_scroll = false;
        self.logs.selected = 0;
        self.logs.scroll_offset = 0;
        self.reset_detail_for_selection();
    }

    pub fn go_bottom(&mut self) {
        self.logs.auto_scroll = true;
        self.new_logs_since_pause = 0;
        // The renderer will snap to bottom on next frame.
        self.reset_detail_for_selection();
    }

    pub fn set_level(&mut self, level: LogLevel) {
        self.filter.min_level = level;
        self.invalidate_filter();
    }
}
