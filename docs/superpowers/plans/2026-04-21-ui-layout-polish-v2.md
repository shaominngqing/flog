# UI Layout Polish v2 Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development or executing-plans. Steps are checkboxes.

**Goal:** Ship iteration 2 from `docs/superpowers/specs/2026-04-21-ui-layout-polish-v2-design.md`: redesign tab bar to pill-style, dedupe LIVE indicator, reposition Jump-to-Bottom, remove ACTIVE card fill, sync Network chrome with Logs, add column header row, add `⇅` to status bar, and polish small issues.

**Architecture:** Pure UI layer. `LayoutCache` adds `col_header_y` and `net_col_header_y`. `ui::tab_bar` rewrites for pill style + no underline. `ui::logs::mod` adds column header. `ui::network` module gets a fresh toolbar matching Logs structure.

**Tech Stack:** Rust, ratatui.

---

## Constraints

- Follow the same strict scope rules as v1: stage only named files per task; no `git add -A`.
- Each task is one focused commit.

---

## File inventory

### Modify
- `src/app.rs` — LayoutCache: add `col_header_y`, `net_col_header_y`.
- `src/ui/tab_bar.rs` — tab bar rewrite: pill-style active tab (1 row), drop LIVE from right side.
- `src/ui/mod.rs` — `draw` dispatcher: tab bar is now 1 row (was 2).
- `src/ui/logs/mod.rs` — `draw_logs` layout (6 rows chrome), add `draw_column_header`, jump-to-bottom vertical position = bottom-4, transparent bg, `T  tag...` spacing, `│` separator between search and tag, Warning-row DEBUG pill fix.
- `src/ui/network/mod.rs` — `draw_network` layout (6 rows chrome), add column header, wire up new toolbar.
- `src/ui/network/filter.rs` — rewrite `draw_network_toolbar` to match Logs 2-row pattern (search+count / pill groups).
- `src/ui/source_select.rs` — ACTIVE app card: remove SURFACE1 fill, keep border/pill/bold.
- `src/event.rs` — update `source_info_x` to include `⇅` prefix when hit-testing.

---

## Task 1: LayoutCache — add col_header fields

**Files:** Modify `src/app.rs`.

- [ ] **Step 1: Add two fields to `LayoutCache` struct**

In `src/app.rs` near other Y-coordinate fields (after `pub toolbar_y: u16,` around line 208), add:
```rust
    /// Y position of the column header row (Logs tab). Set by renderer.
    pub col_header_y: u16,
    /// Y position of the column header row (Network tab). Set by renderer.
    pub net_col_header_y: u16,
```

- [ ] **Step 2: Build**

```bash
cargo build 2>&1 | tail -3
```
Expected: compiles (both fields default-init to 0 via `#[derive(Default)]`).

- [ ] **Step 3: Commit**

```bash
git add src/app.rs
git commit -m "refactor: add col_header_y and net_col_header_y to LayoutCache

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 2: Tab bar — pill-style active tab, drop LIVE, 1 row

**Files:** Modify `src/ui/tab_bar.rs` and `src/ui/mod.rs`.

- [ ] **Step 1: Rewrite `src/ui/tab_bar.rs`**

Replace the entire body of `pub fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect)` with:

```rust
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

    let logs_text = if logs_active { " ▤ Logs " } else { "▤ Logs" };
    let net_text = if net_active { " ⇄ Network " } else { "⇄ Network" };

    let mut spans1: Vec<Span> = Vec::new();

    let logs_start_col = 2usize;
    spans1.push(Span::styled("  ", Style::default().bg(bg))); // 2-space left margin
    spans1.push(Span::styled(
        logs_text.to_string(),
        if logs_active { active_pill } else { inactive_text },
    ));
    let logs_end_col = logs_start_col + logs_text.width();

    spans1.push(Span::styled("  ", Style::default().bg(bg))); // gap between tabs
    let net_start_col = logs_end_col + 2;
    spans1.push(Span::styled(
        net_text.to_string(),
        if net_active { active_pill } else { inactive_text },
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
```

Imports at the top must include: `BLUE, GREEN, MANTLE, MAUVE, OVERLAY0, SUBTEXT0`. Update the existing `use super::{...}` line.

- [ ] **Step 2: Update dispatcher in `src/ui/mod.rs`**

Find `Constraint::Length(2), // tab bar` in `draw()` (around line 160). Change to `Constraint::Length(1), // tab bar`.

Also update `app.layout.tab_bar_y = rows[0].y;` — unchanged.

- [ ] **Step 3: Build**

```bash
cargo build 2>&1 | tail -3
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/tab_bar.rs src/ui/mod.rs
git commit -m "feat(ui): tab bar becomes 1-row pill; remove LIVE from right side

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 3: Logs chrome — 6-row layout + column header + T tag spacing + pipe + warning fix

**Files:** Modify `src/ui/logs/mod.rs`.

- [ ] **Step 1: Update `draw_logs` to 6-row chrome**

Replace the Layout constraints in `draw_logs`:

```rust
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // sep below tab bar
            Constraint::Length(1), // op row 1: search + count
            Constraint::Length(1), // op row 2: T tag + levels
            Constraint::Length(1), // sep below ops
            Constraint::Length(1), // column header
            Constraint::Min(3),    // main
            Constraint::Length(1), // status
        ])
        .split(area);

    app.layout.toolbar_y = rows[1].y;          // op row 1 (for click hit-test on search)
    app.layout.col_header_y = rows[4].y;
    app.layout.bottom_y = rows[6].y;

    draw_separator_rule(f, rows[0]);
    draw_toolbar_op1(f, app, rows[1]);
    draw_toolbar_op2(f, app, rows[2]);
    draw_separator_rule(f, rows[3]);
    draw_column_header(f, rows[4]);

    if app.show_detail_panel {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(100 - app.detail_panel_pct),
                Constraint::Percentage(app.detail_panel_pct),
            ])
            .split(rows[5]);

        app.layout.list_y = cols[0].y;
        app.layout.list_height = cols[0].height;

        draw_log_list(f, app, cols[0]);
        detail::draw_side_panel(f, app, cols[1]);
    } else {
        app.layout.list_y = rows[5].y;
        app.layout.list_height = rows[5].height;

        draw_log_list(f, app, rows[5]);
    }

    draw_jump_to_bottom(f, app, rows[5]);
    draw_status_bar(f, app, rows[6]);
```

- [ ] **Step 2: Split `draw_toolbar` into `draw_toolbar_op1` and `draw_toolbar_op2`**

Delete the existing `draw_toolbar` function. Add two new functions:

```rust
fn draw_toolbar_op1(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let search_active = app.mode == AppMode::Search;
    let w = area.width as u16;

    let mut spans: Vec<Span> = Vec::new();

    spans.push(Span::styled(" ", Style::default().bg(bg)));

    let si = if search_active {
        Style::default().fg(MANTLE).bg(YELLOW)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled("/", si));
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

    // Sparkline inline after search box
    if !app.search.matches.is_empty() {
        let fc = app.filtered_count();
        let spark = search_sparkline(&app.search.matches, fc, 12);
        spans.push(Span::styled(
            format!(" {}", spark),
            Style::default().fg(LAVENDER).bg(bg),
        ));
        let info = format!(" {}/{} ", app.search.match_idx + 1, app.search.matches.len());
        spans.push(Span::styled(info, Style::default().fg(YELLOW).bg(bg)));
        spans.push(Span::styled("<", Style::default().fg(BLUE).bg(bg)));
        spans.push(Span::styled(">", Style::default().fg(BLUE).bg(bg)));
    }

    let used: u16 = spans.iter().map(|s| s.content.width() as u16).sum();

    // Right-align count: " {filtered}/{total} "
    let count_text = format!(" {}/{} ", app.filtered_count(), app.store.len());
    let cw = count_text.width() as u16;
    let pad = w.saturating_sub(used + cw);
    spans.push(Span::styled(" ".repeat(pad as usize), Style::default().bg(bg)));
    spans.push(Span::styled(count_text, Style::default().fg(SUBTEXT0).bg(bg)));

    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)), area);
}

fn draw_toolbar_op2(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let filter_active = app.mode == AppMode::TagFilter;
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // T pill + 2 spaces + tag placeholder (or pills if active)
    let filter_start_x = x;
    if filter_active {
        spans.push(Span::styled("T", Style::default().fg(MANTLE).bg(GREEN)));
        x += 1;
        spans.push(Span::styled("  ", Style::default().bg(bg)));
        x += 2;
        let fw: usize = 14;
        spans.push(Span::styled(
            safe_pad(&format!("{}_", app.tag_filter.input), fw),
            Style::default().fg(TEXT).bg(SURFACE0),
        ));
        x += fw as u16;
    } else if !app.filter.tag_include.is_empty() || !app.filter.tag_exclude.is_empty() {
        spans.push(Span::styled("T", Style::default().fg(MANTLE).bg(GREEN)));
        x += 1;
        spans.push(Span::styled("  ", Style::default().bg(bg)));
        x += 2;
        let pills = tag_pill_spans(&app.filter);
        for p in &pills {
            x += p.content.width() as u16;
        }
        spans.extend(pills);
    } else {
        spans.push(Span::styled("T", Style::default().fg(MANTLE).bg(GREEN)));
        x += 1;
        spans.push(Span::styled("  ", Style::default().bg(bg)));
        x += 2;
        spans.push(Span::styled(
            safe_pad("tag...", 14),
            Style::default().fg(OVERLAY0).bg(bg),
        ));
        x += 14;
    }
    app.layout.filter_x = (filter_start_x, x);

    // Separator
    spans.push(Span::styled("   │   ", Style::default().fg(SURFACE1).bg(bg)));
    x += 7;

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
            Style::default().fg(SURFACE0).bg(bg).add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(level_color(*level)).bg(bg)
        };
        spans.push(Span::styled(format!(" {} ", label), style));
        x += 3;
    }

    if !app.bookmarks.is_empty() {
        let bm = format!("  ●{}", app.bookmarks.len());
        x += bm.width() as u16;
        spans.push(Span::styled(bm, Style::default().fg(YELLOW).bg(bg)));
    }

    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(" ".repeat(rem as usize), Style::default().bg(bg)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)), area);
}

fn draw_column_header(f: &mut Frame, area: Rect) {
    // Columns: cursor(1) + bookmark(2) + TIME(12) + sep(1) + LEVEL(9) + sep(1) + TAG(14) + sep(1) + MESSAGE
    // We emit label headers aligned to those column starts.
    let header_style = Style::default().fg(OVERLAY0).bg(MANTLE);
    let text = format!(
        " {}{}{}{}{}",
        safe_pad("", 3),                         // cursor + bookmark
        safe_pad("TIME", TIME_WIDTH),            // 12
        safe_pad(" LEVEL", LEVEL_WIDTH + 1),     // 10
        safe_pad(" TAG", TAG_WIDTH + 1),         // 15
        " MESSAGE",
    );
    let w = area.width as usize;
    let pad = w.saturating_sub(text.width());
    let line = Line::from(vec![
        Span::styled(text, header_style),
        Span::styled(" ".repeat(pad), Style::default().bg(MANTLE)),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(MANTLE)),
        area,
    );
}
```

- [ ] **Step 3: Jump-to-bottom: bottom-4 position + BASE bg**

Replace `draw_jump_to_bottom` to use `area.y + area.height - pill_h - 1` and `Style::default().bg(BASE)`:

```rust
fn draw_jump_to_bottom(f: &mut Frame, app: &mut App, area: Rect) {
    if !jump::should_show(app.auto_scroll) {
        app.layout.jump_to_bottom_rect = None;
        return;
    }
    if area.height < 5 || area.width < 24 {
        app.layout.jump_to_bottom_rect = None;
        return;
    }

    let label_text = jump::label(app.new_logs_since_pause);
    let pill_w = (label_text.width() as u16 + 2).min(area.width.saturating_sub(4));
    let pill_h: u16 = 3;
    let pill_x = area.x + (area.width.saturating_sub(pill_w)) / 2;
    let pill_y = area.y + area.height.saturating_sub(pill_h + 1);

    let border_style = Style::default().fg(SAPPHIRE).bg(BASE);
    let top = format!("╭{}╮", "─".repeat((pill_w - 2) as usize));
    let bot = format!("╰{}╯", "─".repeat((pill_w - 2) as usize));

    let mid = if app.new_logs_since_pause > 0 {
        let total_inner = (pill_w - 2) as usize;
        let base_text = "  ↓ Jump to bottom  ";
        let new_text = format!("{} new  ", app.new_logs_since_pause);
        let used = base_text.width() + new_text.width();
        let pad = total_inner.saturating_sub(used);
        vec![
            Span::styled("│", border_style),
            Span::styled(base_text.to_string(), Style::default().fg(TEXT).bg(BASE)),
            Span::styled(new_text, Style::default().fg(YELLOW).bg(BASE)),
            Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
            Span::styled("│", border_style),
        ]
    } else {
        let total_inner = (pill_w - 2) as usize;
        let base_text = "  ↓ Jump to bottom  ";
        let pad = total_inner.saturating_sub(base_text.width());
        vec![
            Span::styled("│", border_style),
            Span::styled(base_text.to_string(), Style::default().fg(TEXT).bg(BASE)),
            Span::styled(" ".repeat(pad), Style::default().bg(BASE)),
            Span::styled("│", border_style),
        ]
    };

    let pill_area = Rect::new(pill_x, pill_y, pill_w, pill_h);
    let lines = vec![
        Line::from(Span::styled(top, border_style)),
        Line::from(mid),
        Line::from(Span::styled(bot, border_style)),
    ];
    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(BASE)),
        pill_area,
    );

    app.layout.jump_to_bottom_rect = Some((pill_x, pill_y, pill_w, pill_h));
}
```

- [ ] **Step 4: Warning/Error row level pill contrast fix**

In `draw_log_list`, find the line that computes `level_span`. Change from:

```rust
            let level_span = level_pill(entry.level, row_bg);
```

to:

```rust
            let level_span = level_pill(entry.level, row_bg);
```

Actually the issue is in `level_pill` returning a bg that's close to the row bg on highlighted rows. Modify `level_pill`:

```rust
fn level_pill(level: LogLevel, row_bg: Color) -> Span<'static> {
    let (fg, bg, bold) = level_badge(level);
    // On highlighted rows (error/warning bg), pull level pill bg down to MANTLE for contrast.
    let pill_bg = if (row_bg == ERROR_ROW_BG || row_bg == WARNING_ROW_BG)
        && matches!(level, LogLevel::Debug | LogLevel::Verbose | LogLevel::System)
    {
        MANTLE
    } else {
        bg
    };
    let label = level.as_str();
    let total_pad = LEVEL_WIDTH.saturating_sub(label.len());
    let left_pad = total_pad / 2;
    let right_pad = total_pad - left_pad;
    let text = format!("{}{}{}", " ".repeat(left_pad), label, " ".repeat(right_pad));
    let mut style = Style::default().fg(fg).bg(pill_bg);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    Span::styled(text, style)
}
```

- [ ] **Step 5: Build**

```bash
cargo build 2>&1 | tail -3
cargo test jump::tests 2>&1 | tail -5
```
Expected: build succeeds, 4 tests still pass.

- [ ] **Step 6: Commit**

```bash
git add src/ui/logs/mod.rs
git commit -m "feat(ui): Logs 6-row chrome + column header + Jump bottom-hug + polish

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 4: Network chrome — 6-row layout with new toolbar + column header

**Files:** Modify `src/ui/network/mod.rs`, `src/ui/network/filter.rs`.

- [ ] **Step 1: Rewrite `src/ui/network/filter.rs::draw_network_toolbar` as two functions**

Delete existing `draw_network_toolbar`. Add:

```rust
pub fn draw_network_op1(f: &mut Frame, app: &mut App, area: Rect, count: usize, total: usize) {
    let bg = MANTLE;
    let w = area.width as u16;
    let is_searching = app.network.search_active;

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(" ", Style::default().bg(bg)));

    let si = if is_searching {
        Style::default().fg(MANTLE).bg(YELLOW)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled("/", si));

    let sw: usize = 40;
    let s = if is_searching {
        format!("{}_", app.network.search_input)
    } else if app.network.filter.search_query.is_empty() {
        "filter url...".to_string()
    } else {
        app.network.filter.search_query.clone()
    };
    let ss = if is_searching {
        Style::default().fg(TEXT).bg(SURFACE0)
    } else if !app.network.filter.search_query.is_empty() {
        Style::default().fg(YELLOW).bg(bg)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    app.layout.net_search_x = (1, 1 + 1 + sw as u16);
    spans.push(Span::styled(safe_pad(&s, sw), ss));

    let used: u16 = spans.iter().map(|x| x.content.width() as u16).sum();
    let count_text = format!(" {}/{} ", count, total);
    let cw = count_text.width() as u16;
    let pad = w.saturating_sub(used + cw);
    spans.push(Span::styled(" ".repeat(pad as usize), Style::default().bg(bg)));
    spans.push(Span::styled(count_text, Style::default().fg(SUBTEXT0).bg(bg)));

    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)), area);
}

pub fn draw_network_op2(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;
    app.layout.net_filter_pills.clear();
    app.layout.net_filter_pills_y = area.y;

    spans.push(Span::styled(" ", Style::default().bg(bg)));
    x += 1;

    // Protocol
    let proto = &app.network.filter.protocol;
    let proto_pills: &[(&str, ProtocolFilter, ratatui::style::Color)] = &[
        ("All", ProtocolFilter::All, GREEN),
        ("HTTP", ProtocolFilter::Http, BLUE),
        ("SSE", ProtocolFilter::Sse, GREEN),
        ("WS", ProtocolFilter::Ws, PEACH),
    ];
    for (label, val, color) in proto_pills {
        let selected = *proto == *val;
        let p = pill(label, selected, *color);
        let start = x;
        x += p.content.width() as u16;
        app.layout.net_filter_pills.push((format!("proto:{}", label), start, x));
        spans.push(p);
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1;
    }

    spans.push(Span::styled(" │ ", Style::default().fg(SURFACE1).bg(bg)));
    x += 3;

    // Method
    let method = &app.network.filter.method;
    let method_pills: &[(&str, MethodFilter, ratatui::style::Color)] = &[
        ("All", MethodFilter::All, GREEN),
        ("GET", MethodFilter::Get, GREEN),
        ("POST", MethodFilter::Post, BLUE),
        ("PUT", MethodFilter::Put, PEACH),
        ("DEL", MethodFilter::Delete, RED),
        ("PATCH", MethodFilter::Patch, MAUVE),
    ];
    for (label, val, color) in method_pills {
        let selected = *method == *val;
        let p = pill(label, selected, *color);
        let start = x;
        x += p.content.width() as u16;
        app.layout.net_filter_pills.push((format!("method:{}", label), start, x));
        spans.push(p);
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1;
    }

    spans.push(Span::styled(" │ ", Style::default().fg(SURFACE1).bg(bg)));
    x += 3;

    // Status
    let status = &app.network.filter.status;
    let status_pills: &[(&str, StatusFilter, ratatui::style::Color)] = &[
        ("All", StatusFilter::All, GREEN),
        ("OK", StatusFilter::Ok, GREEN),
        ("Fail", StatusFilter::Fail, RED),
        ("Active", StatusFilter::Active, YELLOW),
        ("Pending", StatusFilter::Pending, OVERLAY0),
    ];
    for (label, val, color) in status_pills {
        let selected = *status == *val;
        let p = pill(label, selected, *color);
        let start = x;
        x += p.content.width() as u16;
        app.layout.net_filter_pills.push((format!("status:{}", label), start, x));
        spans.push(p);
        spans.push(Span::styled(" ", Style::default().bg(bg)));
        x += 1;
    }

    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(" ".repeat(rem as usize), Style::default().bg(bg)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)), area);
}

pub fn draw_network_column_header(f: &mut Frame, area: Rect) {
    let header_style = Style::default().fg(OVERLAY0).bg(MANTLE);
    // Column layout used by draw_network_list — keep in sync.
    // Widths: PROTO(5) space(1) METHOD(7) space(1) URL(flex)  STATUS(7) TIME(7) SIZE(7)
    let w = area.width as usize;
    let right_cluster = " STATUS   TIME    SIZE ";
    let right_w = right_cluster.width();
    let left = " PROTO  METHOD  URL";
    let left_w = left.width();
    let pad = w.saturating_sub(left_w + right_w);
    let line = Line::from(vec![
        Span::styled(left.to_string(), header_style),
        Span::styled(" ".repeat(pad), Style::default().bg(MANTLE)),
        Span::styled(right_cluster.to_string(), header_style),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(MANTLE)),
        area,
    );
}
```

Keep the existing `pill()` helper (same file).

- [ ] **Step 2: Update `src/ui/network/mod.rs::draw_network`**

Find the current layout in `draw_network` and replace with 6-row chrome similar to Logs. Look at existing code for structure hints. The key idea:

```rust
pub fn draw_network(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // sep
            Constraint::Length(1), // op1: search + count
            Constraint::Length(1), // op2: pills
            Constraint::Length(1), // sep
            Constraint::Length(1), // column header
            Constraint::Min(3),    // list (+ optional detail)
            Constraint::Length(1), // status
        ])
        .split(area);

    app.layout.net_toolbar_y = rows[1].y;
    app.layout.net_col_header_y = rows[4].y;

    crate::ui::logs::draw_separator_rule_pub(f, rows[0]);
    let count = app.network.filtered_count(&app.network_store);
    let total = app.network_store.len();
    filter::draw_network_op1(f, app, rows[1], count, total);
    filter::draw_network_op2(f, app, rows[2]);
    crate::ui::logs::draw_separator_rule_pub(f, rows[3]);
    filter::draw_network_column_header(f, rows[4]);

    // ... (keep existing detail-panel split + list drawing + status bar for rows[5] and rows[6])
    // Use existing `draw_network_list` for rows[5] handling; existing `draw_network_status_bar` for rows[6].
}
```

Before you can call `crate::ui::logs::draw_separator_rule_pub`, you need to **expose** the separator rule. In `src/ui/logs/mod.rs`, rename `fn draw_separator_rule` to `pub(crate) fn draw_separator_rule` and re-export as `pub(crate) use draw_separator_rule as draw_separator_rule_pub;` at module root. Or simplest: make a helper in `ui/mod.rs`:

**Simpler path**: Move `draw_separator_rule` to `src/ui/mod.rs` as a pub helper. Then Logs and Network both call `crate::ui::draw_separator_rule(f, area)`.

Pick the simpler path. Edit `src/ui/mod.rs` to add:

```rust
pub fn draw_separator_rule(f: &mut Frame, area: Rect) {
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

Required imports in `ui/mod.rs`: `Paragraph`, `Line`, `Span`, `Rect`, `Frame`. Add what's missing.

Delete `draw_separator_rule` from `src/ui/logs/mod.rs`. Update callers in `logs/mod.rs` to `crate::ui::draw_separator_rule`.

- [ ] **Step 3: Update existing Network list/status bar code to not render chrome**

Inspect `src/ui/network/mod.rs` for any other toolbar calls and remove — only the new chrome from Step 2 should render chrome.

- [ ] **Step 4: Build**

```bash
cargo build 2>&1 | tail -5
```

If errors about status bar Y, adjust. The status bar drawing call should use `rows[6]`.

- [ ] **Step 5: Commit**

```bash
git add src/ui/network/mod.rs src/ui/network/filter.rs src/ui/logs/mod.rs src/ui/mod.rs
git commit -m "feat(ui): Network tab adopts 6-row chrome + column header matching Logs

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 5: Device picker — remove ACTIVE fill

**Files:** Modify `src/ui/source_select.rs`.

- [ ] **Step 1: Locate the ACTIVE card rendering**

In `src/ui/source_select.rs::draw_device_picker`, find the block where `is_active` branches set `card_bg`. Currently it's `SURFACE1` for active.

- [ ] **Step 2: Change `card_bg` for ACTIVE to MANTLE**

Replace the ACTIVE case assignment:
```rust
let card_bg = if is_active { SURFACE1 } else { MANTLE };
```
with:
```rust
let card_bg = MANTLE; // always transparent — borders + pill carry the distinction
```

Keep `border_color = SAPPHIRE` for ACTIVE unchanged.
Keep text `TEXT bold` vs `SUBTEXT0` unchanged.

- [ ] **Step 3: Build**

```bash
cargo build 2>&1 | tail -3
```

- [ ] **Step 4: Commit**

```bash
git add src/ui/source_select.rs
git commit -m "feat(ui): device picker — ACTIVE card removes SURFACE1 fill

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 6: Status bar — prepend ⇅ switch icon

**Files:** Modify `src/ui/logs/mod.rs`, `src/event.rs`.

- [ ] **Step 1: Add `⇅ ` prefix in `draw_status_bar`**

Find the `ctx` string construction (currently `format!("{}{} · {} · :{}", ca.app_name, v, dev, ca.port)`). Change the Span so it includes a SAPPHIRE `⇅ ` prefix:

Replace the ctx Span push block:
```rust
                    Span::styled(
                        ctx,
                        Style::default()
                            .fg(SUBTEXT0)
                            .bg(bg)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
```
with:
```rust
                    Span::styled(
                        "⇅ ".to_string(),
                        Style::default().fg(SAPPHIRE).bg(bg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        ctx,
                        Style::default()
                            .fg(SUBTEXT0)
                            .bg(bg)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
```

Also extend `ctxw` to include `⇅ ` (2 columns — `⇅` is narrow-width + space):
```rust
            let ctxw = (ctx.width() + 2) as u16; // +2 for "⇅ "
```

And update the hit-test region `sx` to cover the icon too:
```rust
            let sx = (lw + cw, lw + cw + ctxw);
```
(Already correct since `ctxw` grew.)

- [ ] **Step 2: Build**

```bash
cargo build 2>&1 | tail -3
```

- [ ] **Step 3: Smoke-confirm the click region still works**

(No code test — the existing event.rs `handle_bottom_click` uses `source_info_x` range, which we just widened. Should still route to Device Picker.)

- [ ] **Step 4: Commit**

```bash
git add src/ui/logs/mod.rs
git commit -m "feat(ui): status bar prepends ⇅ switch icon to app context

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>"
```

---

## Task 7: Final smoke test + build

- [ ] **Step 1: Full build + test**

```bash
cargo build --release 2>&1 | tail -3
cargo test 2>&1 | tail -8
cargo clippy 2>&1 | grep "^error" | head -5
```

Expected: release builds, 108+ tests pass, no errors.

- [ ] **Step 2: Install and run**

```bash
cp target/release/flog ~/.cargo/bin/flog
```

Run against a live Flutter app. Walk through:
1. Tab bar: active tab is BLUE pill; right side shows `[iOS]/[Android]/[Sim] AppName` only.
2. Chrome is 6 rows above content (both Logs and Network).
3. Column header row visible.
4. Jump-to-Bottom hugs bottom (4 rows up), BASE bg, only border visible.
5. Device picker ACTIVE card: no fill, double-line border, GREEN pill.
6. Status bar has `⇅` before app context.
7. `T  tag...` shows T pill + 2 spaces + "tag..." placeholder.
8. Warning rows' DEBUG pills readable.
9. Click on `⇅ AuraLang ...` opens device picker.
10. Network tab matches Logs structurally.

- [ ] **Step 3: If issues found**

Fix inline with `fix(ui): ...` commits.

- [ ] **Step 4: Done**
