//! Event dispatcher — routes keyboard and mouse events to per-mode
//! handlers.
//!
//! # Top-level routing (UI-007)
//!
//! `handle_key` and `handle_mouse` fan out by `AppMode`:
//!
//! ```text
//! AppMode::Normal          → handle_normal_key / handle_normal_mouse
//! AppMode::InputActive(f)  → handle_input_key(f, …)  / handle_input_mouse
//! AppMode::Help | Stats    → handle_overlay_key     / handle_overlay_mouse
//! AppMode::MockRuleEdit    → handle_mock_edit_key   / handle_mock_edit_mouse
//! ```
//!
//! Each sub-handler ends in a `_ => {}` catch-all. **These no-op
//! arms are intentional**: unhandled keys / mouse events in a given
//! mode are swallowed silently (no status message, no error). See
//! individual handlers for their catch-all; audit UI-007 tracks
//! this decision.
//!
//! # Normal-mode secondary dispatch (UI-007)
//!
//! `handle_normal_key` further branches on two pieces of state *in this
//! order*:
//!
//! 1. `app.show_device_picker == true` → device-picker keys (j/k/Enter/
//!    Esc) only. All other keys are ignored. This is a modal overlay.
//! 2. `app.select_mode == true` → any key exits select mode and
//!    discards the event.
//! 3. `app.active_tab == ViewTab::Network` → Network-tab key handler.
//!    Arms after this point are unreachable given Network routing —
//!    comments mark them `// UI-007`.
//! 4. Otherwise (`ViewTab::Logs`) → Logs-tab key handler.
//!
//! # Mouse: two-phase dispatch (UI-009 + UI-041)
//!
//! `handle_normal_mouse` is a thin dispatcher:
//!
//! 1. `detect::detect_click_region(app, x, y) -> Option<ClickRegion>`
//!    — pure, read-only, no side effects.
//! 2. `detect::classify_click(now, x, y, prev)` → `ClickClass::Single`
//!    or `Double`.
//! 3. `apply::apply_click_region(app, region, class, x, y)` — performs
//!    the mutation.
//! 4. Wheel scroll routes through `handle_scroll` which branches by
//!    (picker open? logs detail panel? network detail? tab?).
//!
//! `click_region::ClickRegion` is the semantic enum mapping a clicked
//! pixel to a UI concept (tabs, pills, list rows, detail panels,
//! status-bar buttons, mock rule rows).
//!
//! # Sub-modules
//!
//! - `actions`    — clipboard + replay + mock-from-selected + copy helpers.
//! - `apply`      — mutation phase of mouse dispatch.
//! - `apply_status` — status-bar click handler extracted from apply.
//! - `click_region` — `ClickRegion`, `ClickClass`, `ScrollDir`, `Axis` enums.
//! - `detect`     — pure click-region detection.
//! - `detect_net` — network-tab region detection (split for file-size).
//! - `keys`       — keyboard handlers for Normal / Input / Overlay /
//!   MockRuleEdit modes (plus `handle_mock_edit_mouse`).
//! - `pills`      — named pill labels (SSE/WS) for hit-testing.
//! - `sse_nav`    — pure SSE merged-field j/k index math.

use std::time::Instant;

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{App, AppMode, ViewTab};

mod actions;
mod apply;
mod apply_status;
mod click_region;
mod detect;
mod detect_net;
mod keys;
mod pills;
mod sse_nav;

use click_region::ScrollDir;
use keys::{
    handle_input_key, handle_mock_edit_key, handle_mock_edit_mouse, handle_normal_key,
    handle_overlay_key,
};

const SCROLL_LINES: usize = 3;

const DOUBLE_CLICK_MS: u128 = 400;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_key(app, key),
        AppMode::InputActive(field) => handle_input_key(app, field, key),
        AppMode::Help | AppMode::Stats => handle_overlay_key(app, key),
        AppMode::MockRuleEdit => handle_mock_edit_key(app, key),
    }
}

pub fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    match app.mode.clone() {
        AppMode::Normal => handle_normal_mouse(app, mouse),
        AppMode::InputActive(_) => handle_input_mouse(app, mouse),
        AppMode::Help | AppMode::Stats => handle_overlay_mouse(app, mouse),
        AppMode::MockRuleEdit => handle_mock_edit_mouse(app, mouse),
    }
}

// ══════════════════════════════════════
//  Normal mode — Mouse
// ══════════════════════════════════════

fn handle_normal_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let (x, y) = (mouse.column, mouse.row);
            let now = Instant::now();
            // Logs-tab left-click historically cleared the transient
            // status message. Preserve that before dispatch.
            if app.active_tab == ViewTab::Logs && !app.show_device_picker {
                app.status_message = None;
            }
            if let Some(region) = detect::detect_click_region(app, x, y) {
                let class = detect::classify_click(now, x, y, app.layout.last_click);
                apply::apply_click_region(app, region, class, x, y);
            }
            app.layout.last_click = Some((now, x, y));
        }
        MouseEventKind::Down(MouseButton::Right) => {
            // Device-picker right-click is ignored (characterization:
            // ui_device_picker_right_click_is_ignored).
            if app.show_device_picker {
                return;
            }
            // Logs list row right-click → toggle bookmark.
            let y = mouse.row;
            if app.active_tab == ViewTab::Logs
                && y >= app.layout.list_y
                && y < app.layout.list_y + app.layout.list_height
            {
                handle_list_right_click(app, y);
            }
        }
        MouseEventKind::ScrollUp => handle_scroll(app, &mouse, ScrollDir::Up),
        MouseEventKind::ScrollDown => handle_scroll(app, &mouse, ScrollDir::Down),
        _ => {}
    }
}

/// Wheel-scroll dispatcher. Context-sensitive: routes the scroll to the
/// appropriate viewport (device picker, network detail, list, logs
/// detail panel, logs list).
fn handle_scroll(app: &mut App, mouse: &MouseEvent, dir: ScrollDir) {
    // 1. Device picker overlay.
    if app.show_device_picker {
        match dir {
            ScrollDir::Up => {
                app.device_picker_scroll = app.device_picker_scroll.saturating_sub(1);
            }
            ScrollDir::Down => {
                let max = app.layout.device_picker_total_lines.saturating_sub(1);
                if app.device_picker_scroll < max {
                    app.device_picker_scroll += 1;
                }
            }
        }
        return;
    }

    // 2. Logs detail side-panel scroll.
    if app.active_tab == ViewTab::Logs && app.show_detail_panel {
        let panel_start =
            (app.layout.width as u32 * (100 - app.detail_panel_pct as u32) / 100) as u16;
        if mouse.column >= panel_start
            && mouse.row > app.layout.toolbar_y
            && mouse.row < app.layout.bottom_y
        {
            match dir {
                ScrollDir::Up => app.detail_scroll_up(SCROLL_LINES),
                ScrollDir::Down => app.detail_scroll_down(SCROLL_LINES),
            }
            return;
        }
    }

    // 3. Network detail scroll (right pane).
    if app.active_tab == ViewTab::Network
        && app.network.show_detail
        && mouse.column >= app.layout.net_detail_x
        && mouse.row >= app.layout.list_y
        && mouse.row < app.layout.bottom_y
    {
        match dir {
            ScrollDir::Up => {
                app.network.detail_scroll = app.network.detail_scroll.saturating_sub(SCROLL_LINES);
            }
            ScrollDir::Down => {
                app.network.detail_scroll += SCROLL_LINES;
            }
        }
        return;
    }

    // 4. Tab-specific list scroll.
    match app.active_tab {
        ViewTab::Network => match dir {
            ScrollDir::Up => app.network.move_up(SCROLL_LINES),
            ScrollDir::Down => {
                let count = app.network.filtered_count(&app.network_store);
                app.network.move_down(SCROLL_LINES, count);
            }
        },
        ViewTab::Logs => match dir {
            ScrollDir::Up => app.move_up(SCROLL_LINES),
            ScrollDir::Down => app.move_down(SCROLL_LINES),
        },
    }
}

fn compute_list_target(app: &App, y: u16) -> Option<usize> {
    let row_in_list = (y - app.layout.list_y) as usize;
    // Use the row→entry mapping built by the renderer (handles variable-height entries).
    app.layout.row_to_filtered_idx.get(row_in_list).copied()
}

fn handle_list_click(app: &mut App, y: u16, _is_double: bool) {
    if let Some(target) = compute_list_target(app, y) {
        if app.selected == target {
            // Click same row → toggle panel open/close
            app.show_detail_panel = !app.show_detail_panel;
        } else {
            // Click different row → open panel with new selection
            app.selected = target;
            app.show_detail_panel = true;
            app.reset_detail_for_selection();
        }
    }
}

/// Public-to-module wrapper for apply::apply_click_region.
pub(super) fn handle_list_click_public(app: &mut App, y: u16) {
    handle_list_click(app, y, false);
}

fn handle_list_right_click(app: &mut App, y: u16) {
    if let Some(target) = compute_list_target(app, y) {
        app.selected = target;
        app.toggle_bookmark();
        // Feedback
        if let Some(idx) = app.selected_store_index() {
            if app.is_bookmarked(idx) {
                app.show_status("Bookmarked".to_string());
            } else {
                app.show_status("Bookmark removed".to_string());
            }
        }
    }
}

// ══════════════════════════════════════
//  Input mode — Mouse
// ══════════════════════════════════════

fn handle_input_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = mouse.column;
            let y = mouse.row;
            if y == app.layout.input_row_y {
                use crate::app::InputField;
                if app.active_tab == ViewTab::Logs {
                    if x >= app.layout.log_search_x.0 && x < app.layout.log_search_x.1 {
                        app.enter_input_field(InputField::LogSearch);
                        return;
                    }
                    if x >= app.layout.log_exclude_x.0 && x < app.layout.log_exclude_x.1 {
                        app.enter_input_field(InputField::LogExclude);
                        return;
                    }
                    if x >= app.layout.log_tag_x.0 && x < app.layout.log_tag_x.1 {
                        app.enter_input_field(InputField::LogTag);
                        return;
                    }
                } else {
                    if x >= app.layout.net_search_x.0 && x < app.layout.net_search_x.1 {
                        app.enter_input_field(InputField::NetSearch);
                        return;
                    }
                    if x >= app.layout.net_exclude_x.0 && x < app.layout.net_exclude_x.1 {
                        app.enter_input_field(InputField::NetExclude);
                        return;
                    }
                }
            }
            // Click elsewhere → exit
            app.exit_input_field();
        }
        MouseEventKind::Down(MouseButton::Right) => app.exit_input_field(),
        MouseEventKind::ScrollUp => {
            if app.active_tab == ViewTab::Logs {
                app.move_up(SCROLL_LINES);
            } else {
                app.network.move_up(SCROLL_LINES);
            }
        }
        MouseEventKind::ScrollDown => {
            if app.active_tab == ViewTab::Logs {
                app.move_down(SCROLL_LINES);
            } else {
                let count = app.network.filtered_count(&app.network_store);
                app.network.move_down(SCROLL_LINES, count);
            }
        }
        _ => {}
    }
}

// ══════════════════════════════════════
//  Overlay (Help / Stats) — Mouse
// ══════════════════════════════════════

fn handle_overlay_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let x = mouse.column;
            let y = mouse.row;

            if y == 0 && x < 10 {
                match app.mode {
                    AppMode::Help => app.exit_help(),
                    AppMode::Stats => app.exit_stats(),
                    _ => {}
                }
                return;
            }

            // Stats: clickable slowest requests
            if app.mode == AppMode::Stats && app.active_stats_tab == ViewTab::Network {
                for &(store_idx, ry, x_start, x_end) in &app.layout.stats_slowest_regions {
                    if y == ry && x >= x_start && x < x_end {
                        app.exit_stats();
                        let filtered = app.network.filtered_indices(&app.network_store).to_vec();
                        if let Some(fi) = filtered.iter().position(|&idx| idx == store_idx) {
                            app.network.selected = fi;
                            app.network.auto_scroll = false;
                            app.network.show_detail = true;
                        }
                        return;
                    }
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => match app.mode {
            AppMode::Help => app.exit_help(),
            AppMode::Stats => app.exit_stats(),
            _ => {}
        },
        _ => {}
    }
}

// ══════════════════════════════════════
//  Task 5 structural tests — two-phase dispatch
// ══════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    fn seed_layout(app: &mut App) {
        app.layout.width = 80;
        app.layout.tab_bar_y = 0;
        app.layout.tab_logs_x = (1, 9);
        app.layout.tab_network_x = (10, 21);
        app.layout.toolbar_y = 2;
        app.layout.toolbar_op2_y = 3;
        app.layout.input_row_y = 5;
        app.layout.list_y = 7;
        app.layout.list_height = 5;
        app.layout.bottom_y = 15;
        app.layout.source_info_x = (40, 60);
    }

    fn click(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::empty(),
        }
    }

    fn scroll_up(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: x,
            row: y,
            modifiers: KeyModifiers::empty(),
        }
    }

    fn scroll_down(x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: x,
            row: y,
            modifiers: KeyModifiers::empty(),
        }
    }

    #[test]
    fn tab_bar_double_click_switches_tab_twice() {
        let mut app = App::default();
        seed_layout(&mut app);
        assert_eq!(app.active_tab, ViewTab::Logs);
        // Click Network tab twice: first enters it, second stays on it
        // (switch_tab is idempotent for same tab). Verifies dispatcher
        // does not panic on double-click path.
        handle_mouse(&mut app, click(15, 0));
        handle_mouse(&mut app, click(15, 0));
        assert_eq!(app.active_tab, ViewTab::Network);
    }

    #[test]
    fn scroll_up_in_logs_tab_calls_move_up() {
        let mut app = App::default();
        seed_layout(&mut app);
        // Seed scroll_offset so move_up has something to decrement.
        app.scroll_offset = 10;
        handle_mouse(&mut app, scroll_up(5, 9));
        assert!(app.scroll_offset < 10);
    }

    #[test]
    fn scroll_down_in_logs_tab_does_not_panic() {
        let mut app = App::default();
        seed_layout(&mut app);
        handle_mouse(&mut app, scroll_down(5, 9));
        // No assertion on offset (depends on filtered count being 0);
        // just verify no panic.
    }

    #[test]
    fn click_outside_detail_while_device_picker_open_closes_picker() {
        let mut app = App::default();
        seed_layout(&mut app);
        app.show_device_picker = true;
        app.layout.device_picker_rect = Some((30, 3, 20, 6));
        // Click far outside the picker rect.
        handle_mouse(&mut app, click(1, 1));
        assert!(!app.show_device_picker);
    }
}
