# UI Layout Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the UI refresh described in `docs/superpowers/specs/2026-04-20-ui-layout-polish-design.md` — redesigned top chrome, timeline removal, floating Jump-to-Bottom pill, rebuilt device picker, unified bottom buttons, polished empty states.

**Architecture:** Pure UI layer refactor under `src/ui/`. No domain or transport changes. The renderer continues to be the scroll authority. `LayoutCache` gains one field (`jump_to_bottom_rect`) and loses one (`timeline_y`). Event handlers get updated hit-testing.

**Tech Stack:** Rust, ratatui, Catppuccin Macchiato palette (unchanged).

---

## Spec reference

All section numbers below refer to `docs/superpowers/specs/2026-04-20-ui-layout-polish-design.md`.

## File inventory

### Modify
- `src/app.rs` — `LayoutCache`: add `jump_to_bottom_rect: Option<(u16, u16, u16, u16)>`, remove `timeline_y: u16`.
- `src/ui/mod.rs` — no structural change; remove `timeline` mention from docstring if any.
- `src/ui/logs/mod.rs` — redo `draw_logs` vertical layout (3 chrome rows + list + status), rewrite `draw_toolbar` (drop `flog` pill, add `│` separators, move `15/15` count here), overhaul `draw_status_bar` (left-extend with app/device/port, right unified buttons), rewrite empty state functions (`draw_not_connected`, `draw_waiting_for_logs`, `draw_no_matching_logs`), add `draw_jump_to_bottom`, add new `draw_separator_rule`.
- `src/ui/tab_bar.rs` — render right-side `AppName vX.Y · Device  ● LIVE` context; keep label + underline treatment.
- `src/ui/source_select.rs` — redesign `draw_device_picker` device containers + selection states, update `draw_waiting_for_connection` for new subtitle + Quick Start card.
- `src/event.rs` — update hit-testing to reflect new row positions (no `timeline_y`), add click handler for `jump_to_bottom_rect`.

### Delete
- `src/ui/logs/timeline.rs` — removed entirely after all callers are gone.

### No test file
flog has no existing unit test harness for the UI rendering layer (TUI output is hard to snapshot-test). Tasks include a **manual smoke test step** instead of a `cargo test` step. The one automated test we add is for the new Jump-to-Bottom visibility logic, which is pure boolean state and testable without ratatui.

---

## Task ordering rationale

1. **Layout/state plumbing first** (Task 1) — `LayoutCache` struct fields need to change before renderers can compile.
2. **Remove timeline** (Task 2) — frees 3 rows, simplifies later layout math.
3. **Top chrome rewrite** (Task 3–4) — tab bar context + toolbar.
4. **Empty states** (Task 5) — uses utilities established in Task 3.
5. **Bottom status bar** (Task 6) — standalone.
6. **Jump-to-bottom overlay** (Task 7) — new feature, needs all prior plumbing.
7. **Device picker** (Task 8–9) — self-contained subsystem, done last to avoid blocking.
8. **Smoke test + commit** (Task 10).

Each task produces a compiling, runnable binary.

---

## Task 1: LayoutCache — add `jump_to_bottom_rect`, remove `timeline_y`

**Files:**
- Modify: `src/app.rs:207-271` (LayoutCache struct and its derived Default)
- Modify: `src/event.rs:93, 570-574, 744` (remove timeline_y references — these will be deleted in Task 2's event handler update; for now just flag them)

- [ ] **Step 1: Edit LayoutCache struct**

In `src/app.rs` around line 207, inside `pub struct LayoutCache`:

Remove this line:
```rust
    pub timeline_y: u16,
```

Add (near the other rect fields, e.g. after `pub device_picker_total_lines`):
```rust
    /// Jump-to-bottom floating overlay rect: (x, y, w, h). None when hidden.
    pub jump_to_bottom_rect: Option<(u16, u16, u16, u16)>,
```

- [ ] **Step 2: Verify compile will fail meaningfully**

Run: `cargo check 2>&1 | grep -E "timeline_y|jump_to_bottom"`
Expected: errors about `timeline_y` being used in `src/event.rs` and `src/ui/logs/mod.rs`. We fix those in later tasks.

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "refactor: add jump_to_bottom_rect, remove timeline_y from LayoutCache

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 2: Remove Timeline

**Files:**
- Delete: `src/ui/logs/timeline.rs`
- Modify: `src/ui/logs/mod.rs:6` (remove `pub mod timeline;`)
- Modify: `src/ui/logs/mod.rs:1` (update docstring)
- Modify: `src/ui/logs/mod.rs:153-193` (redo `draw_logs` vertical layout)
- Modify: `src/event.rs:89-95, 568-576, 742-745` (remove timeline hit-testing)

- [ ] **Step 1: Delete the timeline file**

Run:
```bash
rm src/ui/logs/timeline.rs
```

- [ ] **Step 2: Update `src/ui/logs/mod.rs` module imports**

Replace lines 1–6:

```rust
//! Logs view — main log list with toolbar and status bar.

pub mod detail;
pub mod highlight;
pub mod stats;
```

(Removes the `//! ... timeline ...` comment and `pub mod timeline;`.)

- [ ] **Step 3: Update `draw_logs` vertical layout**

In `src/ui/logs/mod.rs`, replace the `draw_logs` function body's layout block (currently lines 152–193). The new body:

```rust
pub fn draw_logs(f: &mut Frame, app: &mut App, area: Rect) {
    // Vertical: tab-context handled upstream; this fn gets everything below the tab bar.
    // Layout: separator | toolbar | main | status
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // separator rule
            Constraint::Length(1), // toolbar
            Constraint::Min(3),    // main area (list + optional detail)
            Constraint::Length(1), // status bar
        ])
        .split(area);

    app.layout.toolbar_y = rows[1].y;
    app.layout.bottom_y = rows[3].y;

    draw_separator_rule(f, rows[0]);
    draw_toolbar(f, app, rows[1]);

    // Main area: detail panel or log list
    if app.show_detail_panel {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100 - app.detail_panel_pct),
                Constraint::Percentage(app.detail_panel_pct),
            ])
            .split(rows[2]);

        app.layout.list_y = cols[0].y;
        app.layout.list_height = cols[0].height;

        draw_log_list(f, app, cols[0]);
        detail::draw_side_panel(f, app, cols[1]);
    } else {
        app.layout.list_y = rows[2].y;
        app.layout.list_height = rows[2].height;

        draw_log_list(f, app, rows[2]);
    }

    // Floating overlay (after list so it layers on top)
    draw_jump_to_bottom(f, app, rows[2]);

    draw_status_bar(f, app, rows[3]);
}
```

Note: `draw_separator_rule` and `draw_jump_to_bottom` are added in later tasks (Task 3 and Task 7). To keep Task 2 compilable, add stubs at the bottom of `src/ui/logs/mod.rs`:

```rust
fn draw_separator_rule(_f: &mut Frame, _area: Rect) {
    // Real implementation in Task 3.
}

fn draw_jump_to_bottom(_f: &mut Frame, _app: &mut App, _area: Rect) {
    // Real implementation in Task 7.
}
```

- [ ] **Step 4: Remove timeline hit-testing in `src/event.rs`**

Find this block around line 89–95:
```rust
            && mouse.row > app.layout.toolbar_y
            && mouse.row < app.layout.timeline_y
```
Replace `app.layout.timeline_y` with `app.layout.bottom_y`.

Find block around 568–576:
```rust
            } else if y >= app.layout.timeline_y && y < app.layout.bottom_y {
                // Timeline click → jump to position
                let fc = app.filtered_count();
                let offset = crate::ui::logs::timeline::click_to_offset(x, app.layout.width, fc);
                ...
            }
```
Delete the entire `else if` branch (leave preceding/following branches intact).

Find block around 742–745:
```rust
    if mouse.row <= app.layout.toolbar_y || mouse.row >= app.layout.timeline_y {
```
Replace with:
```rust
    if mouse.row <= app.layout.toolbar_y || mouse.row >= app.layout.bottom_y {
```

- [ ] **Step 5: Verify compile passes**

Run: `cargo build 2>&1 | tail -20`
Expected: build succeeds (warnings OK).

- [ ] **Step 6: Smoke test**

Run: `cargo run` with a flog_dart app connected. Confirm:
- No timeline strip below the log list.
- Log list fills the vertical space up to the bottom status bar minus one row for the toolbar.
- Mouse wheel / scrolling behaves as before.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(ui): remove timeline heatmap, reclaim 3 rows for log list

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 3: Top chrome — separator rule + tab bar context

**Files:**
- Modify: `src/ui/logs/mod.rs` (implement `draw_separator_rule`)
- Modify: `src/ui/tab_bar.rs` (render right-side context)

- [ ] **Step 1: Implement `draw_separator_rule` in `src/ui/logs/mod.rs`**

Replace the stub added in Task 2:

```rust
fn draw_separator_rule(f: &mut Frame, area: Rect) {
    let rule: String = "─".repeat(area.width as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(SURFACE0).bg(MANTLE),
        )))
        .style(Style::default().bg(MANTLE)),
        area,
    );
}
```

- [ ] **Step 2: Update tab bar to render right-side context**

In `src/ui/tab_bar.rs`, replace the entire `draw_tab_bar` function body. Key additions: after building `spans1` for the left-side tab labels, append a right-aligned context string before the final padding.

Full replacement (preserves click-region registration):

```rust
pub fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 2 {
        return;
    }

    let bg = MANTLE;
    let w = area.width as usize;

    let logs_icon = "▤";
    let net_icon = "⇄";

    let (logs_label_style, logs_icon_style) = if app.active_tab == ViewTab::Logs {
        (
            Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD),
            Style::default().fg(BLUE).bg(bg),
        )
    } else {
        (
            Style::default().fg(OVERLAY0).bg(bg),
            Style::default().fg(OVERLAY0).bg(bg),
        )
    };

    let (net_label_style, net_icon_style) = if app.active_tab == ViewTab::Network {
        (
            Style::default().fg(BLUE).bg(bg).add_modifier(Modifier::BOLD),
            Style::default().fg(BLUE).bg(bg),
        )
    } else {
        (
            Style::default().fg(OVERLAY0).bg(bg),
            Style::default().fg(OVERLAY0).bg(bg),
        )
    };

    let mut spans1: Vec<Span> = vec![
        Span::styled("   ", Style::default().bg(bg)),
        Span::styled(logs_icon, logs_icon_style),
        Span::styled(" Logs", logs_label_style),
        Span::styled("        ", Style::default().bg(bg)),
        Span::styled(net_icon, net_icon_style),
        Span::styled(" Network", net_label_style),
    ];
    let used_left: usize = spans1.iter().map(|s| s.content.width()).sum();

    // ── Right-side context: "AppName vX.Y · Device  ● LIVE" ──
    let active_app = app
        .active_app_id
        .as_ref()
        .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id));

    let mut right_spans: Vec<Span> = Vec::new();
    if let Some(ca) = active_app {
        let app_label = if ca.app_version.is_empty() {
            ca.app_name.clone()
        } else {
            format!("{} v{}", ca.app_name, ca.app_version)
        };
        let device_short = if ca.device_name.is_empty() {
            ca.device_id.clone()
        } else {
            ca.device_name.clone()
        };
        right_spans.push(Span::styled(
            format!("{} · {}", app_label, device_short),
            Style::default().fg(SUBTEXT0).bg(bg),
        ));
        right_spans.push(Span::styled("  ", Style::default().bg(bg)));

        if app.auto_scroll {
            let dot = match (app.tick / 8) % 4 {
                0 => "●",
                1 => "◉",
                2 => "●",
                _ => "○",
            };
            right_spans.push(Span::styled(
                format!(" {} LIVE ", dot),
                Style::default()
                    .fg(MANTLE)
                    .bg(GREEN)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            right_spans.push(Span::styled(
                " PAUSED ".to_string(),
                Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD),
            ));
        }
    }

    let used_right: usize = right_spans.iter().map(|s| s.content.width()).sum();

    // Assemble: left spans + padding + right spans
    let pad = w.saturating_sub(used_left + used_right);
    spans1.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
    spans1.extend(right_spans);

    // ── Line 2: underline under active tab ──
    let logs_start: usize = 3;
    let logs_end: usize = 9;
    let net_start: usize = 17;
    let net_end: usize = 26;

    let mut line2 = String::with_capacity(w);
    for i in 0..w {
        let is_logs_active = app.active_tab == ViewTab::Logs && i >= logs_start && i < logs_end;
        let is_net_active = app.active_tab == ViewTab::Network && i >= net_start && i < net_end;
        if is_logs_active || is_net_active {
            line2.push('─');
        } else {
            line2.push(' ');
        }
    }

    let underline_style = Style::default().fg(BLUE).bg(bg);
    let bg_style = Style::default().bg(bg);
    let mut spans2: Vec<Span> = Vec::new();
    let chars: Vec<char> = line2.chars().collect();
    let mut pos = 0;
    while pos < chars.len() {
        let is_dash = chars[pos] == '─';
        let start = pos;
        while pos < chars.len() && (chars[pos] == '─') == is_dash {
            pos += 1;
        }
        let segment: String = chars[start..pos].iter().collect();
        if is_dash {
            spans2.push(Span::styled(segment, underline_style));
        } else {
            spans2.push(Span::styled(segment, bg_style));
        }
    }

    app.layout.tab_logs_x = (logs_start as u16, logs_end as u16);
    app.layout.tab_network_x = (net_start as u16, net_end as u16);

    let lines = vec![Line::from(spans1), Line::from(spans2)];
    f.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
}
```

Add these imports to the top of `src/ui/tab_bar.rs` if not already present:
```rust
use super::{BLUE, GREEN, MANTLE, OVERLAY0, SUBTEXT0, YELLOW};
```

- [ ] **Step 3: Verify compile passes**

Run: `cargo build 2>&1 | tail -20`
Expected: build succeeds.

- [ ] **Step 4: Smoke test**

Run with an app connected. Confirm:
- Top row shows `▤ Logs  ⇄ Network` on the left and `AppName vX.Y · Device  ● LIVE` on the right.
- Second row shows a SURFACE0-colored `─` line spanning full width (separator rule between tab bar and toolbar).
- Active tab is underlined in BLUE.
- Pausing scroll (move_up) toggles `● LIVE` to `PAUSED` yellow pill.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(ui): tab bar shows app/device context and LIVE state; add separator rule

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 4: Toolbar — drop flog logo, add `│` separators, move `15/15` count

**Files:**
- Modify: `src/ui/logs/mod.rs` (replace `draw_toolbar` function)

- [ ] **Step 1: Replace `draw_toolbar`**

Find `fn draw_toolbar(f: &mut Frame, app: &mut App, area: Rect)` in `src/ui/logs/mod.rs` (currently at line 200). Replace the entire function with:

```rust
fn draw_toolbar(f: &mut Frame, app: &mut App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;
    let bg = MANTLE;
    let search_active = app.mode == AppMode::Search;
    let filter_active = app.mode == AppMode::TagFilter;

    // Leading space
    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Search
    let si = if search_active {
        Style::default().fg(MANTLE).bg(YELLOW)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled("/", si));
    x += 1;
    let sw: usize = 20;
    let st = if search_active {
        format!("{}_", app.search.input)
    } else if app.filter.search_query.is_empty() {
        "search...".into()
    } else {
        app.filter.search_query.clone()
    };
    let ss = if search_active {
        Style::default().fg(TEXT).bg(SURFACE0)
    } else if !app.filter.search_query.is_empty() {
        Style::default().fg(YELLOW).bg(bg)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    app.layout.search_x = (1, 1 + 1 + sw as u16);
    spans.push(Span::styled(safe_pad(&st, sw), ss));
    x += sw as u16;

    // Sparkline
    if !app.search.matches.is_empty() {
        let fc = app.filtered_count();
        let spark = search_sparkline(&app.search.matches, fc, 12);
        spans.push(Span::styled(
            format!(" {}", spark),
            Style::default().fg(LAVENDER).bg(bg),
        ));
        x += 1 + spark.width() as u16;
        let info = format!("{}/{}", app.search.match_idx + 1, app.search.matches.len());
        spans.push(Span::styled(
            format!(" {} ", info),
            Style::default().fg(YELLOW).bg(bg),
        ));
        x += 2 + info.width() as u16;
        spans.push(Span::styled("<", Style::default().fg(BLUE).bg(bg)));
        spans.push(Span::styled("> ", Style::default().fg(BLUE).bg(bg)));
        x += 3;
    }

    spans.push(Span::styled("   ", Style::default().bg(bg)));
    x += 3;

    // Tag filter
    let filter_start_x = x;
    if filter_active {
        spans.push(Span::styled("T", Style::default().fg(MANTLE).bg(GREEN)));
        x += 1;
        let fw: usize = 14;
        spans.push(Span::styled(
            safe_pad(&format!("{}_", app.tag_filter.input), fw),
            Style::default().fg(TEXT).bg(SURFACE0),
        ));
        x += fw as u16;
    } else if !app.filter.tag_include.is_empty() || !app.filter.tag_exclude.is_empty() {
        let pills = tag_pill_spans(&app.filter);
        for p in &pills {
            x += p.content.width() as u16;
        }
        spans.extend(pills);
    } else {
        spans.push(Span::styled("T", Style::default().fg(OVERLAY0).bg(bg)));
        x += 1;
        spans.push(Span::styled(
            safe_pad(" tag...", 7),
            Style::default().fg(OVERLAY0).bg(bg),
        ));
        x += 7;
    }
    app.layout.filter_x = (filter_start_x, x);

    // Separator
    spans.push(Span::styled("  │  ", Style::default().fg(SURFACE1).bg(bg)));
    x += 5;

    // Level buttons — pill style
    app.layout.levels_x = x;
    for (label, level) in &[
        ("S", LogLevel::System),
        ("V", LogLevel::Verbose),
        ("D", LogLevel::Debug),
        ("I", LogLevel::Info),
        ("W", LogLevel::Warning),
        ("E", LogLevel::Error),
    ] {
        let (fg, bg_c, bold) = level_badge(*level);
        let style = if app.filter.min_level == *level {
            let mut s = Style::default()
                .fg(fg)
                .bg(if bg_c == Color::Reset { SURFACE1 } else { bg_c });
            if bold {
                s = s.add_modifier(Modifier::BOLD);
            }
            s
        } else if app.filter.min_level > *level {
            Style::default()
                .fg(SURFACE0)
                .bg(bg)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(level_color(*level)).bg(bg)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        x += 3;
    }

    // Separator + counts
    spans.push(Span::styled("  │  ", Style::default().fg(SURFACE1).bg(bg)));
    x += 5;

    let count_text = format!("{}/{}", app.filtered_count(), app.store.len());
    spans.push(Span::styled(
        count_text.clone(),
        Style::default().fg(SUBTEXT0).bg(bg),
    ));
    x += count_text.width() as u16;

    if !app.bookmarks.is_empty() {
        let bm = format!("  ●{}", app.bookmarks.len());
        x += bm.width() as u16;
        spans.push(Span::styled(bm, Style::default().fg(YELLOW).bg(bg)));
    }

    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(
            " ".repeat(rem as usize),
            Style::default().bg(bg),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

- [ ] **Step 2: Verify compile passes**

Run: `cargo build 2>&1 | tail -10`
Expected: build succeeds.

- [ ] **Step 3: Smoke test**

Run. Confirm:
- Toolbar no longer has the `flog` BLUE pill at the start.
- Two dim `│` separators flank the level buttons.
- `15/15` (filtered/total) appears to the right of the level buttons.
- Search box and tag pills still interactive with hotkeys `/` and `T`.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ui): drop flog logo pill, add toolbar separators, move count into toolbar

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 5: Empty states — waiting for connection, waiting for logs, no match

**Files:**
- Modify: `src/ui/logs/mod.rs` (rewrite `draw_waiting_for_logs`, `draw_no_matching_logs`)
- Modify: `src/ui/source_select.rs` (rewrite `draw_waiting_for_connection`)

- [ ] **Step 1: Rewrite `draw_waiting_for_connection` in `src/ui/source_select.rs`**

Replace the function body (currently starts at `pub fn draw_waiting_for_connection(f: &mut Frame, app: &App, area: Rect)`):

```rust
pub fn draw_waiting_for_connection(f: &mut Frame, app: &App, area: Rect) {
    let tick = app.tick;
    let h = area.height as usize;
    let w = area.width as usize;

    let mut lines: Vec<Line> = Vec::new();
    // Target content height
    let quickstart_h = if app.discovered_devices.is_empty() { 7 } else { 0 };
    let devlist_h = if app.discovered_devices.is_empty() {
        0
    } else {
        app.discovered_devices.len() + 2
    };
    let content_h = BANNER.len() + 6 + quickstart_h + devlist_h;
    let top_pad = h.saturating_sub(content_h) / 3;

    for _ in 0..top_pad {
        lines.push(fill_line(w));
    }

    for (row, text) in BANNER.iter().enumerate() {
        lines.push(render_banner_line(text, row, tick, w));
    }

    lines.push(centered_text_line(
        "Flutter Log Viewer · Network Inspector",
        w,
        OVERLAY0,
    ));
    lines.push(fill_line(w));

    let spinner = braille_spinner(tick);
    let status = format!("{}  Waiting for connection on port {}...", spinner, app.server_port);
    lines.push(centered_text_line(&status, w, TEXT));
    lines.push(fill_line(w));

    if !app.discovered_devices.is_empty() {
        lines.push(centered_text_line("Discovered devices:", w, OVERLAY0));
        for dev in app.discovered_devices.values() {
            let kind = kind_label(&dev.kind);
            let info = format!("  {} ({}) — {}", dev.name, dev.id, kind);
            lines.push(centered_text_line(&info, w, SUBTEXT0));
        }
    } else {
        // Quick Start card: 7 rows
        let box_w: usize = 46;
        let box_left_pad = w.saturating_sub(box_w) / 2;
        let pad = |s: &str| {
            let s_w = s.width();
            let right = box_w.saturating_sub(s_w);
            let mut spans = vec![
                Span::styled(" ".repeat(box_left_pad), Style::default().bg(BASE)),
            ];
            spans.push(Span::styled(s.to_string(), Style::default().fg(SUBTEXT0).bg(BASE)));
            spans.push(Span::styled(" ".repeat(right), Style::default().bg(BASE)));
            let total = box_left_pad + box_w;
            if total < w {
                spans.push(Span::styled(" ".repeat(w - total), Style::default().bg(BASE)));
            }
            Line::from(spans)
        };
        let border_style = Style::default().fg(SURFACE0).bg(BASE);
        let border_line = |left: char, mid: char, right: char| {
            let mut spans = vec![
                Span::styled(" ".repeat(box_left_pad), Style::default().bg(BASE)),
                Span::styled(left.to_string(), border_style),
                Span::styled(mid.to_string().repeat(box_w - 2), border_style),
                Span::styled(right.to_string(), border_style),
            ];
            let total = box_left_pad + box_w;
            if total < w {
                spans.push(Span::styled(" ".repeat(w - total), Style::default().bg(BASE)));
            }
            Line::from(spans)
        };
        let content_line = |text: &str| {
            let inner_w = box_w - 2;
            let text_w = text.width();
            let right = inner_w.saturating_sub(text_w);
            let mut spans = vec![
                Span::styled(" ".repeat(box_left_pad), Style::default().bg(BASE)),
                Span::styled("│", border_style),
                Span::styled(text.to_string(), Style::default().fg(SUBTEXT0).bg(BASE)),
                Span::styled(" ".repeat(right), Style::default().bg(BASE)),
                Span::styled("│", border_style),
            ];
            let total = box_left_pad + box_w;
            if total < w {
                spans.push(Span::styled(" ".repeat(w - total), Style::default().bg(BASE)));
            }
            Line::from(spans)
        };

        lines.push(border_line('┌', '─', '┐'));
        lines.push(content_line("  Quick Start                             "));
        lines.push(content_line("   1. Add flog_dart to your Flutter app   "));
        lines.push(content_line("   2. Run your app in debug mode          "));
        lines.push(content_line("   3. flog will auto-connect              "));
        lines.push(border_line('└', '─', '┘'));
        let _ = pad; // silence unused if refactored later
    }

    while lines.len() < h {
        lines.push(fill_line(w));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}
```

- [ ] **Step 2: Rewrite `draw_waiting_for_logs` in `src/ui/logs/mod.rs`**

Find `fn draw_waiting_for_logs(f: &mut Frame, app: &mut App, area: Rect)` (currently at line ~1075). Replace:

```rust
fn draw_waiting_for_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let tick = app.tick;
    let logo_h = LOGO.len() as u16 + 5;
    let start_y = area.height.saturating_sub(logo_h) / 2;

    let spinner = match (tick / 5) % 8 {
        0 => "⣾",
        1 => "⣽",
        2 => "⣻",
        3 => "⢿",
        4 => "⡿",
        5 => "⣟",
        6 => "⣯",
        _ => "⣷",
    };

    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..start_y {
        lines.push(Line::raw(""));
    }

    for logo_line in logo_lines() {
        lines.push(logo_line);
    }
    lines.push(Line::raw(""));

    // Connected subtitle — show active app info
    let subtitle = app
        .active_app_id
        .as_ref()
        .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id))
        .map(|ca| {
            let version = if ca.app_version.is_empty() {
                String::new()
            } else {
                format!(" v{}", ca.app_version)
            };
            format!("   Connected · {}{} ({})", ca.app_name, version, ca.os)
        })
        .unwrap_or_else(|| "   Flutter Log Viewer".to_string());

    lines.push(Line::from(Span::styled(
        subtitle,
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(format!("   {}  ", spinner), Style::default().fg(BLUE)),
        Span::styled("Waiting for logs...", Style::default().fg(SUBTEXT0)),
    ]));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}
```

- [ ] **Step 3: Rewrite `draw_no_matching_logs` in `src/ui/logs/mod.rs`**

Change the signature to accept `&App` so we can inspect active filters:

Replace the current `fn draw_no_matching_logs(f: &mut Frame, area: Rect)` (line ~1118) with:

```rust
fn draw_no_matching_logs(f: &mut Frame, app: &App, area: Rect) {
    let mid = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();
    for _ in 0..mid.saturating_sub(4) {
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(Span::styled(
        "          \u{2205}",
        Style::default().fg(SURFACE1),
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "    No matching logs",
        Style::default().fg(OVERLAY0),
    )));
    lines.push(Line::from(Span::styled(
        "    Try adjusting filters or level",
        Style::default().fg(SURFACE1),
    )));
    lines.push(Line::raw(""));

    // Active filters card — only render rows that are set
    let mut filter_rows: Vec<String> = Vec::new();
    if !app.filter.search_query.is_empty() {
        filter_rows.push(format!("    search: \"{}\"", app.filter.search_query));
    }
    if app.filter.min_level != LogLevel::System {
        filter_rows.push(format!("    level:  {}+", app.filter.min_level.as_str()));
    }
    let tag_includes: Vec<String> = app
        .filter
        .tag_include
        .iter()
        .map(|t| format!("+{}", t))
        .collect();
    let tag_excludes: Vec<String> = app
        .filter
        .tag_exclude
        .iter()
        .map(|t| format!("-{}", t))
        .collect();
    if !tag_includes.is_empty() || !tag_excludes.is_empty() {
        let combined: Vec<String> = tag_includes.into_iter().chain(tag_excludes).collect();
        filter_rows.push(format!("    tags:   {}", combined.join(" ")));
    }

    if !filter_rows.is_empty() {
        lines.push(Line::from(Span::styled(
            "    ┌─ Active filters ─────────────────┐",
            Style::default().fg(SURFACE0),
        )));
        for r in &filter_rows {
            lines.push(Line::from(vec![
                Span::styled("    │", Style::default().fg(SURFACE0)),
                Span::styled(
                    safe_pad(r, 34),
                    Style::default().fg(SUBTEXT0),
                ),
                Span::styled("│", Style::default().fg(SURFACE0)),
            ]));
        }
        lines.push(Line::from(Span::styled(
            "    └──────────────────────────────────┘",
            Style::default().fg(SURFACE0),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    press esc to clear all",
            Style::default().fg(OVERLAY0),
        )));
    }

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(BASE)), area);
}
```

Update the call site in `draw_log_list` (currently `draw_no_matching_logs(f, area);`) to `draw_no_matching_logs(f, app, area);`.

- [ ] **Step 4: Verify compile passes**

Run: `cargo build 2>&1 | tail -10`
Expected: build succeeds. If `app` is passed as `&mut App` but the function takes `&App`, use `&*app` or adjust the signature.

- [ ] **Step 5: Smoke test**

- Start flog without any Flutter app running — confirm logo, subtitle, spinner, and **Quick Start** card visible.
- Start a Flutter app, let it connect but produce no logs — confirm `Connected · AppName vX.Y (os)` subtitle.
- With logs flowing, type `/` + nonsense and press Enter — confirm "Active filters" card lists the search query, and `esc` hint appears.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(ui): polish empty states — Quick Start card, connected subtitle, active filters

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 6: Bottom status bar — unified buttons, extended left context

**Files:**
- Modify: `src/ui/logs/mod.rs` (rewrite `draw_status_bar`)

- [ ] **Step 1: Replace `draw_status_bar`**

Find `fn draw_status_bar(f: &mut Frame, app: &mut App, area: Rect)` (currently at line ~356). Replace:

```rust
fn draw_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;

    // Left group: toast OR (LIVE pill + counts + app/device/port)
    let (left_spans, left_width, source_x) =
        if let Some(msg) = app.active_status().map(|s| s.to_string()) {
            let ok_text = " OK ";
            let msg_text = format!(" {} ", msg);
            let w = ok_text.width() + msg_text.width();
            (
                vec![
                    Span::styled(
                        ok_text,
                        Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(msg_text, Style::default().fg(TEXT).bg(bg)),
                ],
                w as u16,
                (0u16, 0u16),
            )
        } else {
            let (live_text, live_style) = if app.auto_scroll {
                let dot = match (app.tick / 8) % 4 {
                    0 => "●",
                    1 => "◉",
                    2 => "●",
                    _ => "○",
                };
                (
                    format!(" {} LIVE ", dot),
                    Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
                )
            } else if app.new_logs_since_pause > 0 {
                (
                    format!(" {} new ", app.new_logs_since_pause),
                    Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD),
                )
            } else {
                let total = app.filtered_count();
                let vis = app.layout.visible_entry_count.max(1);
                let max_off = total.saturating_sub(vis);
                let pct = if max_off > 0 {
                    ((app.scroll_offset.min(max_off)) * 100) / max_off
                } else {
                    100
                };
                (
                    format!(" {}% ", pct.min(100)),
                    Style::default().fg(TEXT).bg(SURFACE0),
                )
            };

            let total = app.store.len();
            let filtered = app.filtered_count();
            let counts = format!("  {}/{}  ", filtered, total);

            // Extended context: app vX.Y · device · :port
            let ctx = app
                .active_app_id
                .as_ref()
                .and_then(|id| app.connected_apps.iter().find(|a| &a.id == id))
                .map(|ca| {
                    let v = if ca.app_version.is_empty() {
                        String::new()
                    } else {
                        format!(" v{}", ca.app_version)
                    };
                    let dev = if ca.device_name.is_empty() { &ca.device_id } else { &ca.device_name };
                    format!("{}{} · {} · :{}", ca.app_name, v, dev, ca.port)
                })
                .unwrap_or_default();

            let lw = live_text.width() as u16;
            let cw = counts.width() as u16;
            let ctxw = ctx.width() as u16;
            let sx = (lw + cw, lw + cw + ctxw);
            let w = lw + cw + ctxw;
            (
                vec![
                    Span::styled(live_text, live_style),
                    Span::styled(counts, Style::default().fg(SUBTEXT0).bg(bg)),
                    Span::styled(
                        ctx,
                        Style::default()
                            .fg(SUBTEXT0)
                            .bg(bg)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ],
                w,
                sx,
            )
        };

    app.layout.source_info_x = source_x;

    // Right group: unified MANTLE-bg buttons with SUBTEXT0 label
    let button_style = Style::default().fg(SUBTEXT0).bg(SURFACE0);
    let buttons: Vec<(&str, &str, Style)> = vec![
        ("clear", "  Clear  ", button_style),
        ("export", "  Export  ", button_style),
        ("stats", "  Stats  ", button_style),
        ("help", "  Help  ", button_style),
        ("quit", "  Quit  ", button_style.fg(RED)),
    ];

    let bw: u16 = buttons.iter().map(|(_, l, _)| l.width() as u16 + 1).sum();
    let spacer = area.width.saturating_sub(left_width + bw).max(1);

    let mut spans = left_spans;
    spans.push(Span::styled(
        " ".repeat(spacer as usize),
        Style::default().bg(bg),
    ));

    let mut xc = left_width + spacer;
    app.layout.bottom_buttons.clear();
    for (i, (name, label, style)) in buttons.iter().enumerate() {
        let start = xc;
        spans.push(Span::styled(*label, *style));
        xc += label.width() as u16;
        app.layout.bottom_buttons.push((name, start, xc));
        if i < buttons.len() - 1 {
            spans.push(Span::styled(" ", Style::default().bg(bg)));
            xc += 1;
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}
```

- [ ] **Step 2: Verify compile passes**

Run: `cargo build 2>&1 | tail -10`
Expected: build succeeds.

- [ ] **Step 3: Smoke test**

Confirm:
- Left shows `● LIVE  15/15  AuraLang v1.0.0 · iPhone 17 · :9753`.
- Right shows `Clear  Export  Stats  Help  Quit` — all same SURFACE0 bg, only Quit in RED.
- Clicking each button still triggers the same actions (`clear_logs`, export toast, etc.).
- Click on underlined app context still opens the device picker (source_info_x hit-testing preserved).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ui): unify bottom buttons, extend left status with app/device/port

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 7: Jump-to-Bottom floating overlay

**Files:**
- Modify: `src/ui/logs/mod.rs` (implement `draw_jump_to_bottom`)
- Modify: `src/event.rs` (add click handler)

- [ ] **Step 1: Add a helper test for visibility logic**

Create `src/ui/logs/jump.rs`:

```rust
//! Jump-to-bottom overlay — visibility decision + rendering helpers.

/// Returns true when the overlay should be shown.
/// Rule: shown whenever the list is not at its tail (auto_scroll == false).
pub fn should_show(auto_scroll: bool) -> bool {
    !auto_scroll
}

/// Returns the label text for the pill.
/// - no new logs: "↓ Jump to bottom"
/// - N new since pause: "↓ Jump to bottom  N new"
pub fn label(new_since_pause: usize) -> String {
    if new_since_pause == 0 {
        "  ↓ Jump to bottom  ".to_string()
    } else {
        format!("  ↓ Jump to bottom  {} new  ", new_since_pause)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_when_auto_scroll() {
        assert!(!should_show(true));
    }

    #[test]
    fn shown_when_paused() {
        assert!(should_show(false));
    }

    #[test]
    fn label_no_new() {
        assert_eq!(label(0), "  ↓ Jump to bottom  ");
    }

    #[test]
    fn label_with_new() {
        assert_eq!(label(42), "  ↓ Jump to bottom  42 new  ");
    }
}
```

Add `pub mod jump;` in `src/ui/logs/mod.rs` near the other module declarations (top of file):

```rust
pub mod detail;
pub mod highlight;
pub mod jump;
pub mod stats;
```

- [ ] **Step 2: Run the unit tests**

Run: `cargo test jump::tests -- --nocapture`
Expected: 4 tests pass.

- [ ] **Step 3: Replace the `draw_jump_to_bottom` stub in `src/ui/logs/mod.rs`**

Replace:

```rust
fn draw_jump_to_bottom(f: &mut Frame, app: &mut App, area: Rect) {
    if !jump::should_show(app.auto_scroll) {
        app.layout.jump_to_bottom_rect = None;
        return;
    }
    if area.height < 4 || area.width < 20 {
        app.layout.jump_to_bottom_rect = None;
        return;
    }

    let label_text = jump::label(app.new_logs_since_pause);
    let pill_w = (label_text.width() as u16 + 2).min(area.width.saturating_sub(4));
    let pill_h: u16 = 3;
    let pill_x = area.x + (area.width.saturating_sub(pill_w)) / 2;
    // Vertical position: ~70% down the list area
    let pill_y_rel = (area.height as f32 * 0.70) as u16;
    let pill_y = area.y + pill_y_rel.min(area.height.saturating_sub(pill_h));

    let border_style = Style::default().fg(SAPPHIRE).bg(SURFACE0);
    let fill = " ".repeat((pill_w - 2) as usize);

    // Top border ╭────╮
    let top = format!("╭{}╮", "─".repeat((pill_w - 2) as usize));
    // Middle row
    let mid_parts = if app.new_logs_since_pause > 0 {
        let total_inner = (pill_w - 2) as usize;
        let base = "  ↓ Jump to bottom  ";
        let new_text = format!("{} new  ", app.new_logs_since_pause);
        let used = base.width() + new_text.width();
        let pad = total_inner.saturating_sub(used);
        vec![
            Span::styled("│", border_style),
            Span::styled(base.to_string(), Style::default().fg(TEXT).bg(SURFACE0)),
            Span::styled(new_text, Style::default().fg(YELLOW).bg(SURFACE0)),
            Span::styled(" ".repeat(pad), Style::default().bg(SURFACE0)),
            Span::styled("│", border_style),
        ]
    } else {
        let total_inner = (pill_w - 2) as usize;
        let base = "  ↓ Jump to bottom  ";
        let pad = total_inner.saturating_sub(base.width());
        vec![
            Span::styled("│", border_style),
            Span::styled(base.to_string(), Style::default().fg(TEXT).bg(SURFACE0)),
            Span::styled(" ".repeat(pad), Style::default().bg(SURFACE0)),
            Span::styled("│", border_style),
        ]
    };
    let bot = format!("╰{}╯", "─".repeat((pill_w - 2) as usize));

    let pill_area = Rect::new(pill_x, pill_y, pill_w, pill_h);
    let lines = vec![
        Line::from(Span::styled(top, border_style)),
        Line::from(mid_parts),
        Line::from(Span::styled(bot, border_style)),
    ];
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(SURFACE0)),
        pill_area,
    );
    let _ = fill; // style scaffolding if refactored

    app.layout.jump_to_bottom_rect = Some((pill_x, pill_y, pill_w, pill_h));
}
```

- [ ] **Step 4: Add click handler in `src/event.rs`**

In `src/event.rs`, find the main click handler for `MouseEventKind::Down(MouseButton::Left)`. Before the tab-bar hit-test, add:

```rust
        // Jump-to-bottom pill overlay
        if let Some((px, py, pw, ph)) = app.layout.jump_to_bottom_rect {
            if mouse.row >= py && mouse.row < py + ph && mouse.column >= px && mouse.column < px + pw {
                app.go_bottom();
                return;
            }
        }
```

(`app.go_bottom()` already exists in `src/app.rs` and sets `auto_scroll = true` + clears `new_logs_since_pause`.)

- [ ] **Step 5: Verify compile + unit tests**

Run: `cargo build && cargo test`
Expected: build succeeds, all tests pass (including the 4 new jump tests).

- [ ] **Step 6: Smoke test**

- Scroll up with mouse wheel — confirm the rounded-border `↓ Jump to bottom` pill appears centered, ~70% down.
- Let new logs flow in — confirm the pill updates to show `N new` in YELLOW.
- Click the pill — confirm it snaps to the tail and the pill disappears.
- Press `End` or `G` — same behavior.
- Pill width scales with label content.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(ui): add Jump-to-Bottom floating pill with new-log counter

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 8: Device picker — container redesign (device cards wrap app cards)

**Files:**
- Modify: `src/ui/source_select.rs` (rewrite `draw_device_picker`)

- [ ] **Step 1: Replace `draw_device_picker`**

This is the largest change. The new body (keeping click-region registration logic). Replace the entire function in `src/ui/source_select.rs`:

```rust
pub fn draw_device_picker(f: &mut Frame, app: &mut App, area: Rect) {
    // ── Build device → apps map ──
    let mut device_order: Vec<String> = Vec::new();
    let mut apps_by_device: std::collections::HashMap<String, Vec<&crate::app::ConnectedApp>> =
        std::collections::HashMap::new();
    for ca in &app.connected_apps {
        apps_by_device.entry(ca.device_id.clone()).or_default().push(ca);
        if !device_order.contains(&ca.device_id) {
            device_order.push(ca.device_id.clone());
        }
    }
    for dev in app.discovered_devices.values() {
        if !device_order.contains(&dev.id) {
            device_order.push(dev.id.clone());
        }
    }

    let device_count = device_order.len();

    // ── Modal size ──
    let picker_w = (area.width * 2 / 3).max(60).min(area.width.saturating_sub(4));
    let picker_h = (area.height * 3 / 4).max(8);
    let picker_x = (area.width.saturating_sub(picker_w)) / 2;
    let picker_y = (area.height.saturating_sub(picker_h)) / 2;

    let picker_area = Rect::new(picker_x, picker_y, picker_w, picker_h);
    f.render_widget(Clear, picker_area);

    let title = format!(" Devices ({}) ", device_count);
    let hint = " ↑↓ navigate  ⏎ connect  esc cancel ";
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(SAPPHIRE).add_modifier(Modifier::BOLD))
        .title_bottom(Line::from(Span::styled(
            hint,
            Style::default().fg(OVERLAY0),
        )))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE1))
        .style(Style::default().bg(MANTLE));

    let inner = block.inner(picker_area);

    // ── Empty state ──
    if device_order.is_empty() {
        let iw = inner.width as usize;
        let empty_lines = vec![
            Line::from(Span::styled(" ".repeat(iw), Style::default().bg(MANTLE))),
            centered(iw, "No devices found", OVERLAY0),
            Line::from(Span::styled(" ".repeat(iw), Style::default().bg(MANTLE))),
            centered(iw, "Run your Flutter app with flog_dart", SURFACE1),
        ];
        f.render_widget(Paragraph::new(empty_lines).block(block), picker_area);
        app.layout.device_picker_items = Vec::new();
        app.layout.device_picker_rect = Some((picker_area.x, picker_area.y, picker_area.width, picker_area.height));
        app.layout.device_picker_item_ids = Vec::new();
        app.layout.device_picker_total_lines = 0;
        return;
    }

    // ── Build item list and compute heights ──
    #[derive(Clone)]
    struct DeviceBlock {
        device_id: String,
        device_name: String,
        kind: DeviceKind,
        apps: Vec<AppRow>,
    }
    #[derive(Clone)]
    struct AppRow {
        sel_idx: usize,
        app_id: String,
        is_active: bool,
        app_name: String,
        app_version: String,
        package_name: String,
        os: String,
        build_mode: String,
        port: u16,
    }

    let mut selectable_ids: Vec<String> = Vec::new();
    let mut blocks: Vec<DeviceBlock> = Vec::new();
    for device_id in &device_order {
        let dev = app.discovered_devices.get(device_id);
        let dev_name = dev.map(|d| d.name.clone()).unwrap_or_else(|| device_id.clone());
        let dev_kind = dev.map(|d| d.kind.clone()).unwrap_or(DeviceKind::Local);
        let mut app_rows: Vec<AppRow> = Vec::new();
        if let Some(list) = apps_by_device.get(device_id) {
            for ca in list {
                let sel_idx = selectable_ids.len();
                selectable_ids.push(ca.id.clone());
                app_rows.push(AppRow {
                    sel_idx,
                    app_id: ca.id.clone(),
                    is_active: app.active_app_id.as_deref() == Some(&ca.id),
                    app_name: ca.app_name.clone(),
                    app_version: ca.app_version.clone(),
                    package_name: ca.package_name.clone(),
                    os: ca.os.clone(),
                    build_mode: ca.build_mode.clone(),
                    port: ca.port,
                });
            }
        }
        blocks.push(DeviceBlock {
            device_id: device_id.clone(),
            device_name: dev_name,
            kind: dev_kind,
            apps: app_rows,
        });
    }

    // Clamp selection
    let max_sel = selectable_ids.len().saturating_sub(1);
    if app.device_picker_selected > max_sel {
        app.device_picker_selected = max_sel;
    }

    // Heights: 1 blank + device top border + 1 padding + apps(6 each + 1 between) + 1 waiting/padding + device bottom border
    // For a block:
    //   top_border(1) + top_pad(1) + [(card_h(6) + spacer(1)) per app  |  waiting_row(1)] + bot_pad(1) + bot_border(1)
    let block_height = |b: &DeviceBlock| -> u16 {
        if b.apps.is_empty() {
            1 + 1 + 1 + 1 + 1 // borders + pads + single "Waiting" row
        } else {
            let apps_h: u16 = b.apps.iter().enumerate()
                .map(|(i, _)| if i + 1 == b.apps.len() { 6 } else { 7 })
                .sum();
            1 + 1 + apps_h + 1 + 1
        }
    };
    let total_content_h: u16 = blocks.iter().map(|b| block_height(b) + 1).sum(); // +1 blank between devices
    let visible_h = inner.height;

    // Find Y of selected app card
    let mut selected_y: u16 = 0;
    let mut selected_h: u16 = 6;
    {
        let mut y = 0u16;
        'outer: for b in &blocks {
            y += 1; // blank before block
            y += 2; // top border + top pad
            for (ai, row) in b.apps.iter().enumerate() {
                if row.sel_idx == app.device_picker_selected {
                    selected_y = y;
                    selected_h = 6;
                    break 'outer;
                }
                y += if ai + 1 == b.apps.len() { 6 } else { 7 };
            }
            if b.apps.is_empty() {
                y += 1;
            }
            y += 2; // bot pad + bot border
        }
    }

    let scroll = &mut app.device_picker_scroll;
    let scroll_u16 = *scroll as u16;
    if selected_y < scroll_u16 {
        *scroll = selected_y.saturating_sub(1) as usize;
    } else if selected_y + selected_h > scroll_u16 + visible_h {
        *scroll = (selected_y + selected_h).saturating_sub(visible_h) as usize;
    }
    let scroll_offset = *scroll as u16;

    f.render_widget(block, picker_area);

    let mut click_regions: Vec<(u16, u16, u16, usize)> = Vec::new();
    let mut y: u16 = 0;

    let device_margin: u16 = 2; // indent of device container
    let app_margin: u16 = 4;    // indent of app card (inside device)
    let device_w = inner.width.saturating_sub(device_margin * 2);
    let app_w = inner.width.saturating_sub(app_margin * 2);

    for b in &blocks {
        // blank line before block
        if y + 1 > scroll_offset && y < scroll_offset + visible_h {
            let screen_y = inner.y + y.saturating_sub(scroll_offset);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(" ".repeat(inner.width as usize), Style::default().bg(MANTLE)))),
                Rect::new(inner.x, screen_y, inner.width, 1),
            );
        }
        y += 1;

        // Device container top border with embedded title
        let platform_label = match b.kind {
            DeviceKind::Android => "Android",
            DeviceKind::IosUsb { .. } => "iOS",
            DeviceKind::Local => "Sim",
        };
        let id_display = if b.device_id.len() > 20 {
            format!("{}...{}", &b.device_id[..8], &b.device_id[b.device_id.len()-4..])
        } else {
            b.device_id.clone()
        };
        let conn = match b.kind {
            DeviceKind::Android => "ADB",
            DeviceKind::IosUsb { .. } => "USB",
            DeviceKind::Local => "localhost",
        };
        let title_text = format!(" [{}] {}  {} · {} ", platform_label, b.device_name, conn, id_display);

        let render_device_top = |yy: u16| {
            let screen_y = inner.y + yy.saturating_sub(scroll_offset);
            let total_w = device_w as usize;
            let title_w = title_text.width().min(total_w.saturating_sub(4));
            let left_dashes = 1usize;
            let right_dashes = total_w.saturating_sub(2 + left_dashes + title_w);
            let mut spans = vec![
                Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)),
                Span::styled("┌", Style::default().fg(SURFACE0).bg(MANTLE)),
                Span::styled("─".repeat(left_dashes), Style::default().fg(SURFACE0).bg(MANTLE)),
                Span::styled(
                    safe_truncate_width(&title_text, title_w),
                    Style::default().fg(TEXT).bg(MANTLE).add_modifier(Modifier::BOLD),
                ),
                Span::styled("─".repeat(right_dashes), Style::default().fg(SURFACE0).bg(MANTLE)),
                Span::styled("┐", Style::default().fg(SURFACE0).bg(MANTLE)),
                Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)),
            ];
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            if used < inner.width as usize {
                spans.push(Span::styled(
                    " ".repeat(inner.width as usize - used),
                    Style::default().bg(MANTLE),
                ));
            }
            f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, screen_y, inner.width, 1));
        };
        if y + 1 > scroll_offset && y < scroll_offset + visible_h {
            render_device_top(y);
        }
        y += 1;

        // Device top padding row
        if y + 1 > scroll_offset && y < scroll_offset + visible_h {
            render_device_vbar(f, inner, y, scroll_offset, device_margin, device_w);
        }
        y += 1;

        // App cards or waiting row
        if b.apps.is_empty() {
            if y + 1 > scroll_offset && y < scroll_offset + visible_h {
                let screen_y = inner.y + y.saturating_sub(scroll_offset);
                let inner_w = device_w as usize - 2;
                let txt = "  ○ Waiting for app...";
                let pad = inner_w.saturating_sub(txt.width());
                let mut spans = vec![
                    Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)),
                    Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)),
                    Span::styled(txt.to_string(), Style::default().fg(OVERLAY0).bg(MANTLE)),
                    Span::styled(" ".repeat(pad), Style::default().bg(MANTLE)),
                    Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)),
                    Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)),
                ];
                let used: usize = spans.iter().map(|s| s.content.width()).sum();
                if used < inner.width as usize {
                    spans.push(Span::styled(
                        " ".repeat(inner.width as usize - used),
                        Style::default().bg(MANTLE),
                    ));
                }
                f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, screen_y, inner.width, 1));
            }
            y += 1;
        } else {
            for (ai, row) in b.apps.iter().enumerate() {
                // Render the 6-row app card
                render_app_card(
                    f, inner, y, scroll_offset, app_margin, app_w, device_margin, device_w, row,
                    &mut click_regions, app.device_picker_selected,
                );
                y += 6;
                if ai + 1 < b.apps.len() {
                    // Spacer row inside device container
                    if y + 1 > scroll_offset && y < scroll_offset + visible_h {
                        render_device_vbar(f, inner, y, scroll_offset, device_margin, device_w);
                    }
                    y += 1;
                }
            }
        }

        // Device bottom padding
        if y + 1 > scroll_offset && y < scroll_offset + visible_h {
            render_device_vbar(f, inner, y, scroll_offset, device_margin, device_w);
        }
        y += 1;

        // Device bottom border
        if y + 1 > scroll_offset && y < scroll_offset + visible_h {
            let screen_y = inner.y + y.saturating_sub(scroll_offset);
            let total_w = device_w as usize;
            let mut spans = vec![
                Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)),
                Span::styled("└", Style::default().fg(SURFACE0).bg(MANTLE)),
                Span::styled("─".repeat(total_w - 2), Style::default().fg(SURFACE0).bg(MANTLE)),
                Span::styled("┘", Style::default().fg(SURFACE0).bg(MANTLE)),
                Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)),
            ];
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            if used < inner.width as usize {
                spans.push(Span::styled(
                    " ".repeat(inner.width as usize - used),
                    Style::default().bg(MANTLE),
                ));
            }
            f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, screen_y, inner.width, 1));
        }
        y += 1;
    }

    // Scrollbar
    let total_lines = total_content_h as usize;
    let vis_h = visible_h as usize;
    if total_lines > vis_h {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::default().fg(SAPPHIRE))
            .track_style(Style::default().fg(SURFACE0));
        let max_scroll = total_lines.saturating_sub(vis_h);
        let pos = (app.device_picker_scroll).min(max_scroll);
        let mut state = ScrollbarState::new(max_scroll).position(pos);
        f.render_stateful_widget(scrollbar, inner, &mut state);
    }

    app.layout.device_picker_items = click_regions;
    app.layout.device_picker_rect = Some((picker_area.x, picker_area.y, picker_area.width, picker_area.height));
    app.layout.device_picker_item_ids = selectable_ids;
    app.layout.device_picker_total_lines = total_lines;
}

// ── Helpers for device picker ──

fn centered(w: usize, s: &str, fg: Color) -> Line<'static> {
    let pad_l = w.saturating_sub(s.width()) / 2;
    let pad_r = w.saturating_sub(pad_l + s.width());
    Line::from(vec![
        Span::styled(" ".repeat(pad_l), Style::default().bg(MANTLE)),
        Span::styled(s.to_string(), Style::default().fg(fg).bg(MANTLE)),
        Span::styled(" ".repeat(pad_r), Style::default().bg(MANTLE)),
    ])
}

fn safe_truncate_width(s: &str, max_w: usize) -> String {
    crate::ui::safe_truncate(s, max_w)
}

fn render_device_vbar(
    f: &mut Frame,
    inner: Rect,
    y: u16,
    scroll_offset: u16,
    margin: u16,
    device_w: u16,
) {
    let screen_y = inner.y + y.saturating_sub(scroll_offset);
    let interior_w = device_w as usize - 2;
    let mut spans = vec![
        Span::styled(" ".repeat(margin as usize), Style::default().bg(MANTLE)),
        Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)),
        Span::styled(" ".repeat(interior_w), Style::default().bg(MANTLE)),
        Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)),
        Span::styled(" ".repeat(margin as usize), Style::default().bg(MANTLE)),
    ];
    let used: usize = spans.iter().map(|s| s.content.width()).sum();
    if used < inner.width as usize {
        spans.push(Span::styled(
            " ".repeat(inner.width as usize - used),
            Style::default().bg(MANTLE),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, screen_y, inner.width, 1));
}

fn render_app_card(
    f: &mut Frame,
    inner: Rect,
    y: u16,
    scroll_offset: u16,
    app_margin: u16,
    app_w: u16,
    device_margin: u16,
    device_w: u16,
    row: &AppRow,
    click_regions: &mut Vec<(u16, u16, u16, usize)>,
    selected_idx: usize,
) {
    let visible_h = inner.height;
    let is_selected = row.sel_idx == selected_idx;
    let is_active = row.is_active;

    // Border characters
    let (tl, tr, bl, br, h, v) = if is_active {
        ('╔', '╗', '╚', '╝', '═', '║')
    } else {
        ('┌', '┐', '└', '┘', '─', '│')
    };
    let border_color = if is_active { SAPPHIRE } else { SURFACE0 };
    let card_bg = if is_active { SURFACE1 } else { MANTLE };
    let label_fg = if is_active { TEXT } else { SUBTEXT0 };
    let value_fg = if is_active { TEXT } else { SUBTEXT0 };
    let placeholder_fg = SURFACE1;

    let card_w = app_w;

    // Draw 6 rows: top border, name+active+port, pad, pkg row, platform row, mode row, bottom border
    // Actually 6 rows total: top(1) + name(1) + pkg(1) + platform(1) + mode(1) + bottom(1)

    for r in 0..6u16 {
        let yy = y + r;
        if yy + 1 <= scroll_offset || yy >= scroll_offset + visible_h {
            continue;
        }
        let screen_y = inner.y + yy.saturating_sub(scroll_offset);
        let left_outer = " ".repeat(app_margin as usize);
        let right_outer_w = (device_margin as usize).saturating_add(device_margin as usize); // device inner to inner end
        let mut spans: Vec<Span> = Vec::new();
        // Outer device left pipe + spacing
        spans.push(Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)));
        spans.push(Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)));
        spans.push(Span::styled(
            " ".repeat((app_margin - device_margin - 1) as usize),
            Style::default().bg(MANTLE),
        ));

        match r {
            0 => {
                // Top border with inline tag and Port
                let total_w = card_w as usize;
                let dot = if is_active { "●" } else { "○" };
                let name_part = if row.app_version.is_empty() {
                    format!(" {} {} ", dot, row.app_name)
                } else {
                    format!(" {} {} v{} ", dot, row.app_name, row.app_version)
                };
                let active_tag = if is_active { " [ACTIVE] " } else { "" };
                let port_text = format!(" Port: {} ", row.port);

                let content_w = name_part.width() + active_tag.width() + port_text.width();
                let dashes_between = total_w.saturating_sub(2 + content_w);
                spans.push(Span::styled(tl.to_string(), Style::default().fg(border_color).bg(MANTLE)));
                spans.push(Span::styled(
                    name_part,
                    Style::default().fg(label_fg).bg(MANTLE).add_modifier(Modifier::BOLD),
                ));
                if is_active {
                    spans.push(Span::styled(
                        active_tag.to_string(),
                        Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD),
                    ));
                }
                spans.push(Span::styled(
                    h.to_string().repeat(dashes_between),
                    Style::default().fg(border_color).bg(MANTLE),
                ));
                spans.push(Span::styled(
                    port_text,
                    Style::default().fg(SAPPHIRE).bg(MANTLE),
                ));
                spans.push(Span::styled(tr.to_string(), Style::default().fg(border_color).bg(MANTLE)));
            }
            5 => {
                let total_w = card_w as usize;
                spans.push(Span::styled(bl.to_string(), Style::default().fg(border_color).bg(MANTLE)));
                spans.push(Span::styled(
                    h.to_string().repeat(total_w - 2),
                    Style::default().fg(border_color).bg(MANTLE),
                ));
                spans.push(Span::styled(br.to_string(), Style::default().fg(border_color).bg(MANTLE)));
            }
            _ => {
                // Interior rows: detail rows 1..=3 are r=1,2,3; r=4 is blank padding
                let (label, value) = match r {
                    1 => ("Package ", row.package_name.as_str()),
                    2 => ("Platform", row.os.as_str()),
                    3 => ("Mode    ", row.build_mode.as_str()),
                    _ => ("", ""),
                };
                let total_w = card_w as usize;
                let inner_w = total_w - 2;
                let content: String = if label.is_empty() {
                    " ".repeat(inner_w)
                } else {
                    let (display, fg) = if value.is_empty() {
                        ("unknown".to_string(), placeholder_fg)
                    } else {
                        (value.to_string(), value_fg)
                    };
                    let prefix = format!("    {}  ", label);
                    let used = prefix.width() + display.width();
                    let pad = inner_w.saturating_sub(used);
                    let _ = fg; // fg is used inline below
                    spans.push(Span::styled(v.to_string(), Style::default().fg(border_color).bg(card_bg)));
                    spans.push(Span::styled(prefix, Style::default().fg(OVERLAY0).bg(card_bg)));
                    spans.push(Span::styled(display, Style::default().fg(fg).bg(card_bg)));
                    spans.push(Span::styled(" ".repeat(pad), Style::default().bg(card_bg)));
                    spans.push(Span::styled(v.to_string(), Style::default().fg(border_color).bg(card_bg)));
                    String::new()
                };
                if label.is_empty() {
                    spans.push(Span::styled(v.to_string(), Style::default().fg(border_color).bg(card_bg)));
                    spans.push(Span::styled(content, Style::default().bg(card_bg)));
                    spans.push(Span::styled(v.to_string(), Style::default().fg(border_color).bg(card_bg)));
                }
            }
        }

        // Right-side device pipe + spacing
        spans.push(Span::styled(
            " ".repeat((app_margin - device_margin - 1) as usize),
            Style::default().bg(MANTLE),
        ));
        spans.push(Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)));
        spans.push(Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)));

        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        if used < inner.width as usize {
            spans.push(Span::styled(
                " ".repeat(inner.width as usize - used),
                Style::default().bg(MANTLE),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), Rect::new(inner.x, screen_y, inner.width, 1));

        // Register click region for this card-row
        let card_x_start = inner.x + app_margin;
        let card_x_end = card_x_start + card_w;
        click_regions.push((screen_y, card_x_start, card_x_end, row.sel_idx));
    }

    let _ = (is_selected, right_outer_w); // hover/selection affordance in Task 9
}
```

(This body keeps existing imports; verify `DeviceKind`, `Clear`, `Block`, `Borders`, `BorderType`, `Scrollbar`, `ScrollbarOrientation`, `ScrollbarState` are imported — they already are.)

- [ ] **Step 2: Verify compile passes**

Run: `cargo build 2>&1 | tail -30`
Expected: build succeeds. If unused `is_selected` warnings appear, they are intentional — Task 9 adds selection highlighting.

- [ ] **Step 3: Smoke test**

- Open device picker (click source info).
- Confirm each device has a single-line rounded container wrapping its app cards; app cards no longer escape the device header.
- ACTIVE app has double-line `╔═╗` SAPPHIRE border + GREEN `[ACTIVE]` tag + GREEN `●` dot + `Port: N` right-aligned in top edge.
- Non-active apps have single-line `┌─┐` SURFACE0 border + OVERLAY0 `○` dot.
- `Waiting for app...` shows inside the device container when no apps connected.
- Bottom hint bar `↑↓ navigate  ⏎ connect  esc cancel` appears on the modal bottom border.
- Click on any card selects it (current selection handling still works).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ui): device picker — device containers wrap app cards, fix containment

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 9: Device picker — selection preview (SELECTED ≠ ACTIVE)

**Files:**
- Modify: `src/ui/source_select.rs` (add transient selection cursor to `render_app_card`)

- [ ] **Step 1: Draw a left-edge `▎` cursor bar when `is_selected && !is_active`**

In `render_app_card`, find the outer left padding span construction (the `Span::styled(" ".repeat(app_margin as usize), Style::default().bg(MANTLE))` at the start of each row). Replace with logic that draws `▎` in SAPPHIRE when selected-but-not-active:

```rust
        // Left margin — overlay a ▎ cursor at app_margin-1 column when selected-but-not-active
        spans.push(Span::styled(" ".repeat(device_margin as usize), Style::default().bg(MANTLE)));
        spans.push(Span::styled("│", Style::default().fg(SURFACE0).bg(MANTLE)));
        let interior_left = (app_margin - device_margin - 1) as usize;
        if is_selected && !is_active && interior_left >= 1 {
            spans.push(Span::styled(" ".repeat(interior_left - 1), Style::default().bg(MANTLE)));
            spans.push(Span::styled("▎", Style::default().fg(SAPPHIRE).bg(MANTLE)));
        } else {
            spans.push(Span::styled(" ".repeat(interior_left), Style::default().bg(MANTLE)));
        }
```

(Remove the trailing `let _ = (is_selected, right_outer_w);` line since `is_selected` is now used.)

- [ ] **Step 2: Verify compile passes**

Run: `cargo build 2>&1 | tail -10`
Expected: build succeeds, no unused-variable warnings for `is_selected`.

- [ ] **Step 3: Smoke test**

- Open device picker with 2+ apps connected.
- Use `↑↓` to navigate — confirm a thin SAPPHIRE `▎` bar appears on the left edge of the currently-highlighted-but-not-active card.
- Press `⏎` on a different app — the card flips to double-line `╔═╗` ACTIVE border, the cursor bar disappears (because it's now ACTIVE), and the previously-active card reverts to single-line normal.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(ui): device picker — add selection preview cursor for non-active apps

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 10: Final smoke test + docs

**Files:**
- No code changes.

- [ ] **Step 1: Full build and test**

```bash
cargo build --release
cargo test
cargo clippy 2>&1 | grep -E "warning|error" | head -20
```
Expected: release build succeeds, all tests pass, no new clippy warnings in touched files.

- [ ] **Step 2: Full manual smoke test**

Run the release binary against a real Flutter app. Walk through every surface:

1. **Waiting for connection** (no app connected): banner + `Flutter Log Viewer · Network Inspector` + spinner + Quick Start card.
2. **Tab bar**: `▤ Logs  ⇄ Network` left, `AppName v1.0.0 · Device  ● LIVE` right.
3. **Separator rule**: single SURFACE0 `─` line below tab bar.
4. **Toolbar**: `/search...  T tags  │  S V D I W E  │  15/15`.
5. **Log list**: uses all vertical space between toolbar and status bar (no timeline band).
6. **Jump-to-Bottom pill**: scroll up, see centered rounded pill with `N new` in YELLOW; click or press `End` to return.
7. **Status bar**: `● LIVE  15/15  AuraLang v1.0.0 · iPhone 17 · :9753` left; uniform SURFACE0 buttons right with Quit in RED.
8. **Device picker**: container with title, hint at bottom edge; app cards nested inside device containers (never overflowing); ACTIVE card has `╔═╗` + `[ACTIVE]` + GREEN `●`; navigating shows `▎` cursor.
9. **Empty states**:
   - Apply an impossible filter → "No matching logs" + active filters card + esc hint.
   - Connect a fresh app → `Connected · AppName v1.0.0 (os)` subtitle.
10. **Keyboard hotkeys** unchanged: `/`, `T`, `S/V/D/I/W/E`, `End`, `G`, `m`, `?`, `q`, `Esc`.

- [ ] **Step 3: Final commit if any fixup needed**

If the smoke test uncovered any rough edges, fix inline and commit with message `fix(ui): address smoke-test findings for layout polish`.

- [ ] **Step 4: Done**

The feature is ready for merge into master.

---

## Self-review notes (plan author)

- **Spec coverage**:
  - §3 (top chrome): Tasks 2, 3, 4.
  - §4 (timeline removal): Task 2.
  - §5 (jump-to-bottom): Task 7.
  - §6 (device picker): Tasks 8, 9.
  - §7 (status bar): Task 6.
  - §8 (empty states): Task 5.
  - §9 (palette summary): reflected in individual rendering steps.
- **No placeholders**: every code block is complete; "TBD" only appears in §10 of the spec (out of scope) and does not propagate here.
- **Type consistency**: `jump::should_show` / `jump::label` signatures used consistently in Task 7 step 3. `render_app_card` / `render_device_vbar` / `centered` / `safe_truncate_width` all defined in Task 8 and used only within `source_select.rs`.
- **Risks**:
  - The device picker renderer is large (~300 LoC). If the smoke test in Task 8 reveals alignment drift, fix inline before moving to Task 9.
  - Width assumptions in `render_app_card` (e.g., `app_margin - device_margin - 1`) require `app_margin >= device_margin + 1`; current constants (`device_margin=2`, `app_margin=4`) satisfy this.
