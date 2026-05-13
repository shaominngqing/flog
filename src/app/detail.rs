//! Detail-panel interactions: show/hide, reset on selection change,
//! JSON viewer fold toggle, scroll within the detail panel.

use super::{App, AppMode, FullValueOverlayState};

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

    /// Open the full-value overlay for the string node at `id`.
    ///
    /// Sets `app.mode` to `AppMode::FullValueOverlay`; the overlay renderer
    /// in `ui/full_value_overlay.rs` reads the state on the next frame.
    pub fn enter_full_value_overlay(&mut self, text: String, id: u32) {
        self.mode = AppMode::FullValueOverlay(FullValueOverlayState {
            text,
            node_id: id,
            scroll: 0,
        });
    }

    /// Scroll the full-value overlay down by `n` lines.
    pub fn overlay_scroll_down(&mut self, n: usize) {
        if let AppMode::FullValueOverlay(ref mut state) = self.mode {
            state.scroll = state.scroll.saturating_add(n);
        }
    }

    /// Scroll the full-value overlay up by `n` lines.
    pub fn overlay_scroll_up(&mut self, n: usize) {
        if let AppMode::FullValueOverlay(ref mut state) = self.mode {
            state.scroll = state.scroll.saturating_sub(n);
        }
    }
}

#[cfg(test)]
mod full_value_overlay_tests {
    use super::App;
    use crate::app::AppMode;

    #[test]
    fn enter_full_value_overlay_sets_mode() {
        let mut app = App::default();
        app.enter_full_value_overlay("hello".to_string(), 42);
        assert!(
            matches!(app.mode, AppMode::FullValueOverlay(_)),
            "mode should be FullValueOverlay after enter_full_value_overlay"
        );
    }

    #[test]
    fn enter_full_value_overlay_stores_text_and_node_id() {
        let mut app = App::default();
        app.enter_full_value_overlay("world".to_string(), 7);
        let AppMode::FullValueOverlay(ref state) = app.mode else {
            panic!("expected FullValueOverlay mode");
        };
        assert_eq!(state.text, "world");
        assert_eq!(state.node_id, 7);
        assert_eq!(state.scroll, 0, "initial scroll must be 0");
    }

    #[test]
    fn overlay_scroll_down_increments_scroll() {
        let mut app = App::default();
        app.enter_full_value_overlay("text".to_string(), 0);
        app.overlay_scroll_down(3);
        let AppMode::FullValueOverlay(ref state) = app.mode else {
            panic!("expected FullValueOverlay mode");
        };
        assert_eq!(state.scroll, 3);
    }

    #[test]
    fn overlay_scroll_up_saturates_at_zero() {
        let mut app = App::default();
        app.enter_full_value_overlay("text".to_string(), 0);
        // scroll is 0; going up should clamp at 0
        app.overlay_scroll_up(5);
        let AppMode::FullValueOverlay(ref state) = app.mode else {
            panic!("expected FullValueOverlay mode");
        };
        assert_eq!(state.scroll, 0, "scroll must not underflow");
    }

    #[test]
    fn overlay_scroll_down_then_up() {
        let mut app = App::default();
        app.enter_full_value_overlay("text".to_string(), 0);
        app.overlay_scroll_down(10);
        app.overlay_scroll_up(3);
        let AppMode::FullValueOverlay(ref state) = app.mode else {
            panic!("expected FullValueOverlay mode");
        };
        assert_eq!(state.scroll, 7);
    }

    #[test]
    fn scroll_helpers_noop_when_not_in_overlay_mode() {
        let mut app = App::default();
        assert_eq!(app.mode, AppMode::Normal);
        // Should not panic even when mode is Normal
        app.overlay_scroll_down(5);
        app.overlay_scroll_up(5);
        assert_eq!(app.mode, AppMode::Normal, "mode must remain Normal");
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
