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
