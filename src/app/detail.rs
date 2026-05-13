//! Detail-panel interactions: show/hide, reset on selection change,
//! JSON viewer fold toggle, scroll within the detail panel.

use super::App;

impl App {
    pub fn toggle_detail_panel(&mut self) {
        self.show_detail_panel = !self.show_detail_panel;
    }

    pub fn reset_detail_for_selection(&mut self) {
        self.detail.scroll = 0;
        self.detail.viewer_state = crate::ui::json_viewer::JsonViewerState::default();
        self.detail.viewer_tree = None;
        self.detail.viewer_click_map.clear();
        self.detail.viewer_cursor = None;
    }

    /// Move the JSON viewer cursor down one row, clamped to the last row.
    pub fn detail_cursor_down(&mut self) {
        let max = self.detail.viewer_click_map.len().saturating_sub(1);
        self.detail.viewer_cursor = Some(match self.detail.viewer_cursor {
            None => 0,
            Some(i) => i.saturating_add(1).min(max),
        });
    }

    /// Move the JSON viewer cursor up one row, clamped to zero.
    pub fn detail_cursor_up(&mut self) {
        self.detail.viewer_cursor = match self.detail.viewer_cursor {
            None => None,
            Some(0) => Some(0),
            Some(i) => Some(i - 1),
        };
    }

    pub fn toggle_detail_fold(&mut self, node_id: u32) {
        if let Some(ref tree) = self.detail.viewer_tree {
            crate::ui::json_viewer::toggle(tree, &mut self.detail.viewer_state, node_id);
        }
    }

    pub fn detail_scroll_up(&mut self, n: usize) {
        self.detail.scroll = self.detail.scroll.saturating_sub(n);
    }

    pub fn detail_scroll_down(&mut self, n: usize) {
        self.detail.scroll += n;
    }

    /// Stub — real full-value overlay is implemented in Task 5.
    pub fn enter_full_value_overlay(&mut self, text: String, id: u32) {
        let _ = (text, id); // no-op until Task 5
    }
}

#[cfg(test)]
mod detail_cursor_tests {
    use super::App;

    /// Populate viewer_click_map with `n` empty rows so cursor bounds are
    /// exercised without a real renderer.
    fn app_with_click_map(n: usize) -> App {
        let mut app = App::default();
        app.detail.viewer_click_map = vec![Vec::new(); n];
        app
    }

    #[test]
    fn detail_cursor_down_from_none_activates_at_zero() {
        let mut app = app_with_click_map(5);
        assert_eq!(app.detail.viewer_cursor, None);
        app.detail_cursor_down();
        assert_eq!(app.detail.viewer_cursor, Some(0));
    }

    #[test]
    fn detail_cursor_down_increments_normally() {
        let mut app = app_with_click_map(5);
        app.detail.viewer_cursor = Some(2);
        app.detail_cursor_down();
        assert_eq!(app.detail.viewer_cursor, Some(3));
    }

    #[test]
    fn detail_cursor_down_clamps_at_max() {
        let mut app = app_with_click_map(5);
        // max = len - 1 = 4
        app.detail.viewer_cursor = Some(4);
        app.detail_cursor_down();
        assert_eq!(
            app.detail.viewer_cursor,
            Some(4),
            "cursor must not exceed len-1"
        );
    }

    #[test]
    fn detail_cursor_up_from_zero_stays_at_zero() {
        let mut app = app_with_click_map(5);
        app.detail.viewer_cursor = Some(0);
        app.detail_cursor_up();
        assert_eq!(app.detail.viewer_cursor, Some(0));
    }

    #[test]
    fn detail_cursor_up_from_none_stays_none() {
        let mut app = app_with_click_map(5);
        app.detail_cursor_up();
        assert_eq!(app.detail.viewer_cursor, None);
    }

    #[test]
    fn detail_cursor_up_decrements_normally() {
        let mut app = app_with_click_map(5);
        app.detail.viewer_cursor = Some(3);
        app.detail_cursor_up();
        assert_eq!(app.detail.viewer_cursor, Some(2));
    }

    #[test]
    fn reset_detail_for_selection_clears_cursor() {
        let mut app = app_with_click_map(5);
        app.detail.viewer_cursor = Some(3);
        app.reset_detail_for_selection();
        assert_eq!(
            app.detail.viewer_cursor, None,
            "reset_detail_for_selection must clear viewer_cursor"
        );
    }
}
