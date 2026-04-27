//! Multi-line text editor component — state only, no rendering.
//!
//! Manages lines of text with cursor tracking, editing operations,
//! and viewport scrolling. The renderer sets `visible_height` each
//! frame and reads the public fields to draw.
//!
//! Split across submodules (Phase 3 UI-014 mirror):
//! * [`cursor`]   — insert/delete/paste + cursor movement
//! * [`viewport`] — scroll offset adjustment to keep the cursor in view
//! * `mod.rs`     — struct definition, constructors, content accessors

mod cursor;
mod viewport;

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

    // ── Phase 2.5B Task 10b additions ────────────────────────────────
    //
    // Fill remaining uncovered branches.

    #[test]
    fn ensure_cursor_visible_zero_height_no_op() {
        let mut ed = TextEditor::new("a\nb\nc\nd");
        ed.visible_height = 0;
        ed.cursor_row = 3;
        ed.ensure_cursor_visible();
        // No change with zero visible height.
        assert_eq!(ed.scroll_offset, 0);
    }

    #[test]
    fn paste_empty_string_is_single_empty_fragment() {
        // split("\n") on "" yields [""] — which is len==1 but not is_empty.
        // Covers the "parts.len() == 1" branch where the fragment is "".
        let mut ed = TextEditor::new("abc");
        ed.cursor_col = 1;
        ed.paste("");
        // Nothing inserted; cursor stays at col 1.
        assert_eq!(ed.lines, vec!["abc"]);
        assert_eq!(ed.cursor_col, 1);
    }

    #[test]
    fn paste_multiline_three_parts_middle_inserted() {
        // Exercises the "middle fragment" branch (i != 0, i != last).
        let mut ed = TextEditor::new("xyz");
        ed.cursor_col = 1;
        ed.paste("A\nB\nC");
        assert_eq!(ed.lines, vec!["xA", "B", "Cyz"]);
        assert_eq!(ed.cursor_row, 2);
        assert_eq!(ed.cursor_col, 1);
    }

    #[test]
    fn backspace_with_multibyte_char() {
        let mut ed = TextEditor::new("a中b");
        // "中" is 3 bytes UTF-8.
        ed.cursor_col = 4; // after "中"
        ed.backspace();
        assert_eq!(ed.lines, vec!["ab"]);
        assert_eq!(ed.cursor_col, 1);
    }

    #[test]
    fn move_right_across_multibyte() {
        let mut ed = TextEditor::new("a中b");
        ed.cursor_col = 0;
        ed.move_right();
        assert_eq!(ed.cursor_col, 1); // after 'a'
        ed.move_right();
        assert_eq!(ed.cursor_col, 4); // after '中' (3 bytes)
        ed.move_right();
        assert_eq!(ed.cursor_col, 5); // after 'b'
    }

    #[test]
    fn click_with_wide_char_maps_screen_col_to_byte() {
        // "a中b": screen cols are [a=0, 中=1..3, b=3..4]
        let mut ed = TextEditor::new("a中b");
        ed.click(0, 1); // inside 中
                        // Only 'a' consumed before reaching screen_col 1: byte_offset=1
        assert_eq!(ed.cursor_col, 1);
        ed.click(0, 3); // after 中
                        // a (1 byte, w=1) -> 中 (3 bytes, w=2 so screen_col=3 after it)
        assert_eq!(ed.cursor_col, 4);
    }

    #[test]
    fn insert_multibyte_char() {
        let mut ed = TextEditor::new("ab");
        ed.cursor_col = 1;
        ed.insert_char('中');
        assert_eq!(ed.lines, vec!["a中b"]);
        // cursor moves by len_utf8=3
        assert_eq!(ed.cursor_col, 4);
    }

    #[test]
    fn move_left_at_start_no_change() {
        let mut ed = TextEditor::new("abc");
        ed.move_left();
        assert_eq!(ed.cursor_col, 0);
        assert_eq!(ed.cursor_row, 0);
    }

    #[test]
    fn move_left_across_multibyte() {
        // The move_left char-boundary branch (cursor_col > 0) was exercised by
        // test_cursor_movement only via wrap; here we explicitly step back over
        // a multi-byte codepoint.
        let mut ed = TextEditor::new("a中b");
        ed.cursor_col = 4; // after 中
        ed.move_left();
        assert_eq!(ed.cursor_col, 1); // now before 中
        ed.move_left();
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn move_end_on_empty_line() {
        let mut ed = TextEditor::new("");
        ed.move_end();
        assert_eq!(ed.cursor_col, 0);
    }

    #[test]
    fn delete_with_multibyte_char() {
        let mut ed = TextEditor::new("中a");
        ed.cursor_col = 0;
        ed.delete();
        assert_eq!(ed.lines, vec!["a"]);
    }
}
