//! Viewport scroll logic for [`TextEditor`].
//!
//! The renderer writes `visible_height` each frame; this module adjusts
//! `scroll_offset` so the cursor always lands inside the window
//! `[scroll_offset, scroll_offset + visible_height)`.

use super::TextEditor;

impl TextEditor {
    /// Adjust `scroll_offset` so that `cursor_row` is within the visible
    /// window `[scroll_offset, scroll_offset + visible_height)`.
    pub fn ensure_cursor_visible(&mut self) {
        if self.visible_height == 0 {
            return;
        }
        if self.cursor_row < self.scroll_offset {
            self.scroll_offset = self.cursor_row;
        } else if self.cursor_row >= self.scroll_offset + self.visible_height {
            self.scroll_offset = self.cursor_row - self.visible_height + 1;
        }
    }
}
