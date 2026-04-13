//! Multi-line text editor component — state only, no rendering.
//!
//! Manages lines of text with cursor tracking, editing operations,
//! and viewport scrolling. The renderer sets `visible_height` each
//! frame and reads the public fields to draw.

/// A multi-line text editor that tracks cursor position, lines of text,
/// and viewport scroll offset. It provides editing primitives (insert,
/// delete, paste) and cursor movement but does not render anything.
pub struct TextEditor {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_offset: usize,
    pub visible_height: usize,
}

impl TextEditor {
    /// Create a new editor from the given text.
    /// The text is split on `\n`. An empty input yields one empty line.
    pub fn new(text: &str) -> Self {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.split('\n').map(|s| s.to_string()).collect()
        };
        Self {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            visible_height: 10,
        }
    }

    /// Join all lines with `\n`.
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    /// Total number of lines.
    pub fn total_lines(&self) -> usize {
        self.lines.len()
    }

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

    // ── Viewport ─────────────────────────────────────────────

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

    // ── Helpers ──────────────────────────────────────────────

    /// Clamp `cursor_col` to the length of the current line.
    fn clamp_col(&mut self) {
        let len = self.lines[self.cursor_row].len();
        if self.cursor_col > len {
            self.cursor_col = len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let ed = TextEditor::new("");
        assert_eq!(ed.lines, vec![""]);
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn test_new_multiline() {
        let ed = TextEditor::new("a\nb\nc");
        assert_eq!(ed.lines, vec!["a", "b", "c"]);
        assert_eq!(ed.total_lines(), 3);
    }

    #[test]
    fn test_insert_char() {
        // Insert at beginning
        let mut ed = TextEditor::new("hello");
        ed.insert_char('X');
        assert_eq!(ed.lines[0], "Xhello");
        assert_eq!(ed.cursor_col, 1);

        // Insert at middle
        let mut ed = TextEditor::new("hello");
        ed.cursor_col = 2;
        ed.insert_char('X');
        assert_eq!(ed.lines[0], "heXllo");
        assert_eq!(ed.cursor_col, 3);

        // Insert at end
        let mut ed = TextEditor::new("hello");
        ed.cursor_col = 5;
        ed.insert_char('X');
        assert_eq!(ed.lines[0], "helloX");
        assert_eq!(ed.cursor_col, 6);
    }

    #[test]
    fn test_insert_newline() {
        let mut ed = TextEditor::new("abcdef");
        ed.cursor_col = 3;
        ed.insert_char('\n');
        assert_eq!(ed.lines, vec!["abc", "def"]);
        assert_eq!(ed.cursor_row, 1);
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn test_backspace_middle() {
        let mut ed = TextEditor::new("hello");
        ed.cursor_col = 3;
        ed.backspace();
        assert_eq!(ed.lines[0], "helo");
        assert_eq!(ed.cursor_col, 2);
    }

    #[test]
    fn test_backspace_line_merge() {
        let mut ed = TextEditor::new("abc\ndef");
        ed.cursor_row = 1;
        ed.cursor_col = 0;
        ed.backspace();
        assert_eq!(ed.lines, vec!["abcdef"]);
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 3);
    }

    #[test]
    fn test_backspace_at_start_of_first_line() {
        let mut ed = TextEditor::new("hello");
        ed.cursor_col = 0;
        ed.backspace();
        // Nothing should change.
        assert_eq!(ed.lines, vec!["hello"]);
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn test_delete_at_cursor() {
        let mut ed = TextEditor::new("hello");
        ed.cursor_col = 1;
        ed.delete();
        assert_eq!(ed.lines[0], "hllo");
        assert_eq!(ed.cursor_col, 1);
    }

    #[test]
    fn test_delete_line_merge() {
        let mut ed = TextEditor::new("abc\ndef");
        ed.cursor_row = 0;
        ed.cursor_col = 3; // end of "abc"
        ed.delete();
        assert_eq!(ed.lines, vec!["abcdef"]);
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 3);
    }

    #[test]
    fn test_delete_at_end_of_last_line() {
        let mut ed = TextEditor::new("hello");
        ed.cursor_col = 5;
        ed.delete();
        // Nothing should change.
        assert_eq!(ed.lines, vec!["hello"]);
    }

    #[test]
    fn test_cursor_movement() {
        let mut ed = TextEditor::new("abc\ndefgh\nij");

        // move_right across chars
        ed.move_right();
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 1));
        ed.move_right();
        ed.move_right();
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 3));

        // move_right wraps to next line
        ed.move_right();
        assert_eq!((ed.cursor_row, ed.cursor_col), (1, 0));

        // move_down
        ed.move_down();
        assert_eq!((ed.cursor_row, ed.cursor_col), (2, 0));

        // move_down at last line — no change
        ed.move_down();
        assert_eq!((ed.cursor_row, ed.cursor_col), (2, 0));

        // move_up clamps col
        ed.cursor_col = 5; // past end of "ij" (len=2), won't clamp yet
        ed.move_up(); // to line 1 "defgh" (len=5)
        assert_eq!((ed.cursor_row, ed.cursor_col), (1, 5));
        ed.move_up(); // to line 0 "abc" (len=3), col clamped
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 3));

        // move_up at first line — no change
        ed.move_up();
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 3));

        // move_left wraps to end of previous line
        ed.cursor_row = 1;
        ed.cursor_col = 0;
        ed.move_left();
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 3));

        // move_left at (0, 0) — no change
        ed.cursor_row = 0;
        ed.cursor_col = 0;
        ed.move_left();
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 0));

        // move_home / move_end
        ed.cursor_col = 2;
        ed.move_home();
        assert_eq!(ed.cursor_col, 0);
        ed.move_end();
        assert_eq!(ed.cursor_col, 3);
    }

    #[test]
    fn test_click_clamp() {
        let ed_text = "ab\ncde";
        let mut ed = TextEditor::new(ed_text);

        // Click within bounds.
        ed.click(0, 1);
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 1));

        // Click beyond last row — clamp to last line.
        ed.click(99, 0);
        assert_eq!(ed.cursor_row, 1);

        // Click beyond line length — clamp to end of line.
        ed.click(0, 99);
        assert_eq!((ed.cursor_row, ed.cursor_col), (0, 2));

        // Both clamped.
        ed.click(100, 100);
        assert_eq!((ed.cursor_row, ed.cursor_col), (1, 3));
    }

    #[test]
    fn test_paste_multiline() {
        let mut ed = TextEditor::new("hello world");
        ed.cursor_col = 5; // after "hello"
        ed.paste("\nfoo\nbar");
        assert_eq!(ed.lines, vec!["hello", "foo", "bar world"]);
        assert_eq!(ed.cursor_row, 2);
        assert_eq!(ed.cursor_col, 3); // after "bar"
    }

    #[test]
    fn test_paste_single_line() {
        let mut ed = TextEditor::new("abcd");
        ed.cursor_col = 2;
        ed.paste("XY");
        assert_eq!(ed.lines, vec!["abXYcd"]);
        assert_eq!(ed.cursor_col, 4);
    }

    #[test]
    fn test_scroll() {
        let mut ed = TextEditor::new("0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11");
        ed.visible_height = 5;

        // Cursor at row 0, scroll_offset should be 0.
        ed.ensure_cursor_visible();
        assert_eq!(ed.scroll_offset, 0);

        // Move cursor beyond visible area.
        ed.cursor_row = 7;
        ed.ensure_cursor_visible();
        assert_eq!(ed.scroll_offset, 3); // 7 - 5 + 1 = 3

        // Move cursor back above scroll_offset.
        ed.cursor_row = 1;
        ed.ensure_cursor_visible();
        assert_eq!(ed.scroll_offset, 1);

        // Cursor within visible range — no change.
        ed.scroll_offset = 1;
        ed.cursor_row = 3;
        ed.ensure_cursor_visible();
        assert_eq!(ed.scroll_offset, 1);
    }

    #[test]
    fn test_to_string() {
        let ed = TextEditor::new("abc\ndef\nghi");
        assert_eq!(ed.content(), "abc\ndef\nghi");

        let ed = TextEditor::new("");
        assert_eq!(ed.content(), "");
    }

    #[test]
    fn test_lines_never_empty() {
        let mut ed = TextEditor::new("a");
        ed.cursor_col = 1;
        ed.backspace();
        assert_eq!(ed.lines, vec![""]);
        assert!(ed.total_lines() >= 1);
    }
}
