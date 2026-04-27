//! Cursor + editing operations for [`TextEditor`].
//!
//! Contains the insert/delete/paste primitives and cursor movement
//! (left/right/up/down/home/end/click). Every mutation ends with
//! [`TextEditor::ensure_cursor_visible`] so the viewport tracks the
//! caret. The viewport logic itself lives in [`super::viewport`].

use super::TextEditor;

impl TextEditor {
    // ── Editing ──────────────────────────────────────────────

    /// Insert a character at the cursor position.
    /// If the character is `\n`, the current line is split at the cursor.
    pub fn insert_char(&mut self, ch: char) {
        if ch == '\n' {
            let tail = self.lines[self.cursor_row][self.cursor_col..].to_string();
            self.lines[self.cursor_row].truncate(self.cursor_col);
            self.cursor_row += 1;
            self.lines.insert(self.cursor_row, tail);
            self.cursor_col = 0;
        } else {
            let mut s = String::new();
            s.push(ch);
            self.lines[self.cursor_row].insert_str(self.cursor_col, &s);
            self.cursor_col += ch.len_utf8();
        }
        self.ensure_cursor_visible();
    }

    /// Delete the character before the cursor (backspace).
    /// At column 0, merges the current line onto the end of the previous line.
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            // Find the byte index of the char just before cursor_col.
            let line = &self.lines[self.cursor_row];
            let prev_char_boundary = line[..self.cursor_col]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.lines[self.cursor_row].remove(prev_char_boundary);
            self.cursor_col = prev_char_boundary;
        } else if self.cursor_row > 0 {
            let removed = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&removed);
        }
        self.ensure_cursor_visible();
    }

    /// Delete the character at the cursor (forward delete).
    /// At end of line, merges the next line onto this one.
    pub fn delete(&mut self) {
        let line_len = self.lines[self.cursor_row].len();
        if self.cursor_col < line_len {
            self.lines[self.cursor_row].remove(self.cursor_col);
        } else if self.cursor_row + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next);
        }
        self.ensure_cursor_visible();
    }

    /// Paste multi-line text at the cursor position.
    /// The first fragment merges onto the current line at cursor_col;
    /// remaining lines are inserted after.
    pub fn paste(&mut self, text: &str) {
        let parts: Vec<&str> = text.split('\n').collect();
        if parts.is_empty() {
            return;
        }

        // Save the tail of the current line after the cursor.
        let tail = self.lines[self.cursor_row][self.cursor_col..].to_string();
        self.lines[self.cursor_row].truncate(self.cursor_col);

        // Merge the first fragment onto the current line.
        self.lines[self.cursor_row].push_str(parts[0]);

        if parts.len() == 1 {
            // Single-line paste: append the tail back.
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&tail);
        } else {
            // Multi-line paste: insert middle lines, last line gets the tail.
            for (i, part) in parts.iter().enumerate().skip(1) {
                self.cursor_row += 1;
                if i == parts.len() - 1 {
                    // Last fragment — append tail.
                    let mut last_line = part.to_string();
                    self.cursor_col = last_line.len();
                    last_line.push_str(&tail);
                    self.lines.insert(self.cursor_row, last_line);
                } else {
                    self.lines.insert(self.cursor_row, part.to_string());
                }
            }
        }
        self.ensure_cursor_visible();
    }

    // ── Cursor Movement ──────────────────────────────────────

    /// Move cursor left. Wraps to end of previous line if at column 0.
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            // Move to previous char boundary.
            let line = &self.lines[self.cursor_row];
            let prev = line[..self.cursor_col]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor_col = prev;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor right. Wraps to start of next line if at end of line.
    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor_row].len();
        if self.cursor_col < line_len {
            // Move to next char boundary.
            let line = &self.lines[self.cursor_row];
            let next = line[self.cursor_col..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_col + i)
                .unwrap_or(line_len);
            self.cursor_col = next;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor up. Clamps column to the length of the target line.
    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_col();
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor down. Clamps column to the length of the target line.
    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.clamp_col();
        }
        self.ensure_cursor_visible();
    }

    /// Move cursor to the beginning of the current line.
    pub fn move_home(&mut self) {
        self.cursor_col = 0;
        self.ensure_cursor_visible();
    }

    /// Move cursor to the end of the current line.
    pub fn move_end(&mut self) {
        self.cursor_col = self.lines[self.cursor_row].len();
        self.ensure_cursor_visible();
    }

    /// Set cursor to the given (row, col), clamping to valid bounds.
    pub fn click(&mut self, row: usize, col: usize) {
        self.cursor_row = row.min(self.lines.len() - 1);
        // Convert screen column to byte offset, accounting for wide chars
        let line = &self.lines[self.cursor_row];
        let mut byte_offset = 0;
        let mut screen_col = 0;
        for ch in line.chars() {
            if screen_col >= col {
                break;
            }
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
            screen_col += w;
            byte_offset += ch.len_utf8();
        }
        self.cursor_col = byte_offset;
        self.ensure_cursor_visible();
    }

    // ── Helpers ──────────────────────────────────────────────

    /// Clamp `cursor_col` to the length of the current line.
    pub(super) fn clamp_col(&mut self) {
        let len = self.lines[self.cursor_row].len();
        if self.cursor_col > len {
            self.cursor_col = len;
        }
    }
}
