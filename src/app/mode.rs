//! Mode-switch helpers: help, stats, tab switching. Mock-related mode
//! transitions live in [`super::mock_edit`].

use super::{App, AppMode, ViewTab};

impl App {
    pub fn switch_tab(&mut self, tab: ViewTab) {
        self.active_tab = tab;
    }

    /// Returns the auto-scroll flag for the given tab (audit UI-006).
    ///
    /// Routes to the per-tab viewport state: `logs.auto_scroll` for Logs,
    /// `network.auto_scroll` for Network.
    //
    // `#[allow(dead_code)]`: the binary (src/main.rs) compiles each `mod`
    // privately and so does not see the integration-test call sites in
    // tests/*.rs. The helper is exercised by characterization_app_state.rs.
    #[allow(dead_code)]
    pub fn auto_scroll_for_tab(&self, tab: ViewTab) -> bool {
        match tab {
            ViewTab::Logs => self.logs.auto_scroll,
            ViewTab::Network => self.network.auto_scroll,
        }
    }

    pub fn enter_help(&mut self) {
        self.mode = AppMode::Help;
        self.layout.last_click = None;
    }
    pub fn exit_help(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
    }
    pub fn enter_stats(&mut self) {
        self.mode = AppMode::Stats;
        self.active_stats_tab = ViewTab::Logs;
        self.layout.last_click = None;
        // Snapshot stats on entry to prevent flickering from live data changes
        self.stats_snapshot = Some(self.compute_stats());
    }
    pub fn enter_network_stats(&mut self) {
        self.mode = AppMode::Stats;
        self.active_stats_tab = ViewTab::Network;
        self.layout.last_click = None;
    }
    pub fn exit_stats(&mut self) {
        self.mode = AppMode::Normal;
        self.layout.last_click = None;
        self.stats_snapshot = None;
    }

    // ── Bookmarks ──

    pub fn toggle_bookmark(&mut self) {
        if let Some(idx) = self.selected_store_index() {
            if !self.bookmarks.remove(&idx) {
                self.bookmarks.insert(idx);
            }
        }
    }

    pub fn is_bookmarked(&self, store_idx: usize) -> bool {
        self.bookmarks.contains(&store_idx)
    }
}
