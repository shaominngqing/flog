//! `ClickRegion` enum — the pure-function output of mouse click detection.
//!
//! Phase 3 Step 3.6 introduces a two-phase mouse dispatch:
//!
//! 1. `detect::detect_click_region(app, x, y) -> Option<ClickRegion>`
//!    — a read-only function that maps a mouse coordinate to a semantic
//!    region in the UI. This is pure and testable without mutating app
//!    state.
//! 2. `apply::apply_click_region(app, region, class)` — performs all
//!    mutations corresponding to the detected region.
//!
//! Before this split, `handle_normal_mouse` interleaved detection with
//! side effects across ~700 lines (audit UI-009, UI-041), making
//! characterization-style testing impractical for many paths.
//!
//! Variants are `pub(crate)` — internal only, not a serialized wire
//! format. Fields carry the minimum state `apply_click_region` needs so
//! the enum can be `Clone` without borrowing from `App`.

use crate::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use crate::domain::LogLevel;
use crate::ui::json_viewer::JsonAction;

/// Vertical scroll direction for wheel / arrow / pagination primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // variants wired up incrementally in Tasks 3–5
pub(crate) enum ScrollDir {
    Up,
    Down,
}

/// Axis for scrollbar regions (currently only `Vertical` in practice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Axis {
    Vertical,
    Horizontal,
}

/// Single vs. double click classification. Produced by `classify_click`
/// based on the `(time, x, y)` of the previous click and passed through
/// to `apply_click_region` so side effects can branch on it (e.g. mock
/// rule row single-click selects, double-click opens the editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // wired up in Task 4 (classify_click)
pub(crate) enum ClickClass {
    Single,
    Double,
}

/// Semantic click target. One variant per distinguishable region in the
/// Logs or Network view. `detect_click_region` walks layout rects/tables
/// in priority order and returns the first match as one of these
/// variants; `apply_click_region` matches on it to perform mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ClickRegion {
    // ── Device picker overlay ─────────────────────────────────────────
    DevicePickerOutside,
    DevicePickerItem { index: usize },
    DevicePickerScroll { direction: ScrollDir },

    // ── Tab bar ───────────────────────────────────────────────────────
    LogsTab,
    NetworkTab,

    // ── Logs view ─────────────────────────────────────────────────────
    LogsToolbarLevel(LogLevel),
    LogsToolbarSearch,
    LogsToolbarTag,
    LogsToolbarExclude,
    LogsListRow { row: u16 },
    LogsJumpToBottom,
    LogsDetailPanel { line_idx: usize, x: u16 },
    LogsDetailClose,

    // ── Network view ──────────────────────────────────────────────────
    NetworkToolbarSearch,
    NetworkToolbarExclude,
    NetworkProtocolPill(ProtocolFilter),
    NetworkMethodPill(MethodFilter),
    NetworkStatusPill(StatusFilter),
    NetworkMockRulesBtn,
    NetworkListRow { row: u16 },
    NetworkDetailPanel { line_idx: usize, x: u16 },
    NetworkDetailSseEventsPill,
    NetworkDetailSseMergedPill,
    NetworkDetailSseFieldPill { idx: usize },
    NetworkDetailWsChatPill,
    NetworkDetailSectionToggle { section_key: String },
    NetworkDetailMockBtn,
    NetworkDetailReplayBtn,
    NetworkDetailClose,

    // ── Mock rules side panel ─────────────────────────────────────────
    MockRuleRow { index: usize },
    MockRuleEditBtn { index: usize },
    MockRuleToggle { index: usize },
    MockRuleDelete { index: usize },
    MockRuleAdd,
    MockRuleClose,

    // ── Detail panel JSON actions ─────────────────────────────────────
    LogsDetailJsonAction(JsonAction),
    NetworkDetailJsonAction(JsonAction),

    // ── Status bar / other ────────────────────────────────────────────
    StatusBar,
    Scrollbar { axis: Axis, direction: ScrollDir },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_tab_equality_is_variant_based() {
        let a = ClickRegion::LogsTab;
        let b = ClickRegion::LogsTab;
        assert_eq!(a, b);
        assert_ne!(a, ClickRegion::NetworkTab);
    }

    #[test]
    fn logs_tab_is_cloneable() {
        let a = ClickRegion::LogsTab;
        let b = a.clone();
        assert_eq!(a, b);

        // Clone also works for payload-bearing variants.
        let row = ClickRegion::NetworkListRow { row: 7 };
        let row2 = row.clone();
        assert_eq!(row, row2);
    }

    #[test]
    fn logs_tab_debug_contains_variant_name() {
        let dbg = format!("{:?}", ClickRegion::LogsTab);
        assert!(dbg.contains("LogsTab"), "debug was: {dbg}");

        let dbg2 = format!("{:?}", ClickClass::Double);
        assert!(dbg2.contains("Double"), "debug was: {dbg2}");

        let dbg3 = format!("{:?}", ScrollDir::Up);
        assert!(dbg3.contains("Up"), "debug was: {dbg3}");
    }
}
