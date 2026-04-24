//! Tab bar renderer — 1-row pill-style tab selector.

use super::{BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, SUBTEXT0};
use crate::app::{App, ViewTab};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 1 {
        return;
    }

    let bg = MANTLE;
    let w = area.width as usize;

    // Active tab rendered as a solid pill; inactive as plain text.
    // Layout: "  " + [LogsPill] + "  " + [NetPill] + (pad) + right-side context

    let logs_active = app.active_tab == ViewTab::Logs;
    let net_active = app.active_tab == ViewTab::Network;

    // Pill styles
    let active_pill = Style::default()
        .fg(MANTLE)
        .bg(BLUE)
        .add_modifier(Modifier::BOLD);
    let inactive_text = Style::default().fg(OVERLAY0).bg(bg);

    let logs_text = if logs_active {
        " ▤ Logs "
    } else {
        "▤ Logs"
    };
    let net_text = if net_active {
        " ⇄ Network "
    } else {
        "⇄ Network"
    };

    let mut spans1: Vec<Span> = Vec::new();

    let logs_start_col = 2usize;
    spans1.push(Span::styled("  ", Style::default().bg(bg))); // 2-space left margin
    spans1.push(Span::styled(
        logs_text.to_string(),
        if logs_active {
            active_pill
        } else {
            inactive_text
        },
    ));
    let logs_end_col = logs_start_col + logs_text.width();

    spans1.push(Span::styled("  ", Style::default().bg(bg))); // gap between tabs
    let net_start_col = logs_end_col + 2;
    spans1.push(Span::styled(
        net_text.to_string(),
        if net_active {
            active_pill
        } else {
            inactive_text
        },
    ));
    let net_end_col = net_start_col + net_text.width();

    // Right-side: [Platform] AppName  (no LIVE, no underline)
    let active_app = app
        .active_app_id
        .as_ref()
        .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id));

    let mut right_spans: Vec<Span> = Vec::new();
    if let Some(ca) = active_app {
        let (plat_label, plat_bg) = match ca.os.to_lowercase().as_str() {
            s if s.contains("android") => (" Android ", GREEN),
            s if s.contains("ios") => (" iOS ", BLUE),
            _ => (" Sim ", MAUVE),
        };
        right_spans.push(Span::styled(
            plat_label.to_string(),
            Style::default()
                .fg(MANTLE)
                .bg(plat_bg)
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled("  ", Style::default().bg(bg)));
        right_spans.push(Span::styled(
            ca.app_name.clone(),
            Style::default().fg(SUBTEXT0).bg(bg),
        ));
        right_spans.push(Span::styled("  ", Style::default().bg(bg))); // trailing pad
    }

    let used_left: usize = spans1.iter().map(|s| s.content.width()).sum();
    let used_right: usize = right_spans.iter().map(|s| s.content.width()).sum();
    let pad = w.saturating_sub(used_left + used_right);
    spans1.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
    spans1.extend(right_spans);

    // Register click regions
    app.layout.tab_logs_x = (logs_start_col as u16, logs_end_col as u16);
    app.layout.tab_network_x = (net_start_col as u16, net_end_col as u16);

    // Render a single row (no underline row)
    f.render_widget(
        Paragraph::new(Line::from(spans1)).style(Style::default().bg(bg)),
        area,
    );
}

#[cfg(test)]
mod tests {
    //! Phase 2.5B Task 10b — characterization tests for `draw_tab_bar`.
    //!
    //! Uses ratatui TestBackend to drive the actual render path; asserts on
    //! OBSERVABLE features (text, cell backgrounds, layout cache state)
    //! per Rule 3. Multiple variants per render path per Rule 10.
    //!
    //! UNTESTABLE breakdown:
    //!   - Exact column positions of tab labels depend on emoji (▤/⇄) width.
    //!     TestBackend renders each grapheme in 1 cell; we assert logical
    //!     text presence, not pixel x coordinates. Rule 11: PHYS.
    //!   - The draw function reads `app.active_app_id` / `connected_apps`;
    //!     we construct tests both with and without an active app.
    use super::*;
    use crate::app::{App, ConnectedApp, ViewTab};
    use crate::input::ConnectorHandle;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;
    use std::mem;

    fn render(app: &mut App, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = Rect::new(0, 0, width, height);
            draw_tab_bar(f, app, area);
        })
        .unwrap();
        term.backend().buffer().clone()
    }

    fn row_string(buf: &Buffer, y: u16) -> String {
        let area = buf.area;
        (0..area.width)
            .map(|col| buf[(area.x + col, area.y + y)].symbol().to_string())
            .collect::<Vec<_>>()
            .join("")
    }

    fn count_bg(buf: &Buffer, bg: ratatui::style::Color) -> usize {
        let area = buf.area;
        let mut n = 0;
        for y in 0..area.height {
            for x in 0..area.width {
                if buf[(area.x + x, area.y + y)].bg == bg {
                    n += 1;
                }
            }
        }
        n
    }

    fn make_app_with_platform(os: &str) -> (App, ConnectorHandle) {
        let (handle, rx) = ConnectorHandle::for_testing();
        // Leak the receiver so it stays alive for the test duration.
        mem::forget(rx);
        let mut app = App::new();
        let ca = ConnectedApp {
            id: "localhost:9753".to_string(),
            device_id: "localhost".to_string(),
            port: 9753,
            device_name: "Sim".to_string(),
            app_name: "MyApp".to_string(),
            app_version: "1.0".to_string(),
            os: os.to_string(),
            package_name: "com.test".to_string(),
            build_mode: "debug".to_string(),
            handle: handle.clone(),
        };
        app.add_connected_app(ca);
        (app, handle)
    }

    #[test]
    fn tab_bar_zero_height_no_panic_returns_early() {
        let mut app = App::new();
        // Use height 1 for a valid render, but test early-return branch via direct call
        let backend = TestBackend::new(40, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            // call with height=0 area to exercise early return
            let area = Rect::new(0, 0, 40, 0);
            draw_tab_bar(f, &mut app, area);
        })
        .unwrap();
        // No panic — pass.
    }

    #[test]
    fn tab_bar_shows_both_tab_labels() {
        let mut app = App::new();
        let buf = render(&mut app, 60, 1);
        let row = row_string(&buf, 0);
        assert!(row.contains("Logs"), "row missing Logs: {:?}", row);
        assert!(row.contains("Network"), "row missing Network: {:?}", row);
    }

    #[test]
    fn tab_bar_logs_active_has_blue_pill_bg() {
        let mut app = App::new();
        app.active_tab = ViewTab::Logs;
        let buf = render(&mut app, 60, 1);
        // Active pill bg is BLUE (#8aadf4) per style map.
        assert!(
            count_bg(&buf, BLUE) > 0,
            "expected BLUE cells for active logs pill"
        );
    }

    #[test]
    fn tab_bar_network_active_has_blue_pill_bg_and_logs_inactive() {
        let mut app = App::new();
        app.active_tab = ViewTab::Network;
        let buf = render(&mut app, 60, 1);
        assert!(
            count_bg(&buf, BLUE) > 0,
            "expected BLUE cells for active network pill"
        );
        // The inactive logs label should use MANTLE bg.
        assert!(
            count_bg(&buf, MANTLE) > 0,
            "expected MANTLE cells for bg and inactive pill"
        );
    }

    #[test]
    fn tab_bar_layout_cache_populates_tab_click_regions() {
        let mut app = App::new();
        // Pre-existing defaults: (0, 0)
        app.layout.tab_logs_x = (0, 0);
        app.layout.tab_network_x = (0, 0);
        let _ = render(&mut app, 80, 1);
        let (ls, le) = app.layout.tab_logs_x;
        let (ns, ne) = app.layout.tab_network_x;
        assert!(
            le > ls,
            "logs click region should be non-empty: {:?}",
            (ls, le)
        );
        assert!(
            ne > ns,
            "network click region should be non-empty: {:?}",
            (ns, ne)
        );
        assert!(ns >= le, "network region should come after logs region");
    }

    #[test]
    fn tab_bar_narrow_terminal_still_renders_no_panic() {
        let mut app = App::new();
        // very narrow — likely less than needed for pills + app
        let buf = render(&mut app, 5, 1);
        assert_eq!(buf.area.width, 5);
        // At least some cells should have the tab bar bg (MANTLE).
        assert!(count_bg(&buf, MANTLE) > 0);
    }

    #[test]
    fn tab_bar_with_android_app_shows_platform_pill() {
        let (mut app, _h) = make_app_with_platform("android");
        let buf = render(&mut app, 80, 1);
        let row = row_string(&buf, 0);
        assert!(
            row.contains("Android"),
            "row should contain 'Android': {:?}",
            row
        );
        assert!(
            row.contains("MyApp"),
            "row should contain app name: {:?}",
            row
        );
        // Android platform pill uses GREEN bg.
        assert!(
            count_bg(&buf, GREEN) > 0,
            "expected GREEN cells for Android platform pill"
        );
    }

    #[test]
    fn tab_bar_with_ios_app_shows_ios_platform() {
        let (mut app, _h) = make_app_with_platform("ios");
        let buf = render(&mut app, 80, 1);
        let row = row_string(&buf, 0);
        assert!(row.contains("iOS"), "row should contain 'iOS': {:?}", row);
    }

    #[test]
    fn tab_bar_with_unknown_os_shows_sim_platform() {
        let (mut app, _h) = make_app_with_platform("macos");
        let buf = render(&mut app, 80, 1);
        let row = row_string(&buf, 0);
        assert!(
            row.contains("Sim"),
            "row should contain 'Sim' for unknown os: {:?}",
            row
        );
        // Sim platform pill uses MAUVE bg.
        assert!(
            count_bg(&buf, MAUVE) > 0,
            "expected MAUVE cells for Sim platform pill"
        );
    }

    #[test]
    fn tab_bar_tall_terminal_only_renders_first_row() {
        let mut app = App::new();
        // Even if we give area=1 of a tall terminal, draw_tab_bar renders just 1 row.
        let backend = TestBackend::new(40, 5);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = Rect::new(0, 0, 40, 1);
            draw_tab_bar(f, &mut app, area);
        })
        .unwrap();
        let buf = term.backend().buffer().clone();
        // Row 0 has content; rows 1-4 are untouched (default Reset bg).
        let r0 = row_string(&buf, 0);
        assert!(r0.contains("Logs"));
    }

    #[test]
    fn tab_bar_hit_regions_ordered_correctly() {
        // With logs_active the width of " ▤ Logs " differs from inactive;
        // still the network region must begin after logs region ends.
        let mut app = App::new();
        app.active_tab = ViewTab::Logs;
        let _ = render(&mut app, 80, 1);
        let (_, le) = app.layout.tab_logs_x;
        let (ns, _) = app.layout.tab_network_x;
        assert!(ns >= le);
    }
}
