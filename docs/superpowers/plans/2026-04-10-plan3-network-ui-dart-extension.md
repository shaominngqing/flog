# Plan 3: Network View UI + Dart flog_logger Extension

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the full Network Inspector UI (toolbar, request table, detail panel with HTTP/SSE/WS support, mouse interactions) and extend the Dart flog_logger package with FlogHttpInterceptor, FlogSseParser, and FlogWebSocket.

**Architecture:** Two independent workstreams — the Rust Network UI and the Dart package extension. The Rust side builds `src/ui/network/` (mod.rs, detail.rs, filter.rs) with full rendering for all three protocol types. The Dart side adds three classes to `flog_logger/` that emit the `[INFO][flog_net]` protocol. Both sides can be developed in parallel since they communicate through the protocol format defined in Plan 2.

**Tech Stack:** Rust (ratatui 0.29), Dart (pure Dart, no Flutter SDK)

**Depends on:** Plan 2 (Dual-Tab Architecture) must be completed first.

---

### Task 1: Network Toolbar Renderer

**Files:**
- Create: `src/ui/network/filter.rs`
- Modify: `src/ui/network/mod.rs`

- [ ] **Step 1: Create `src/ui/network/filter.rs`**

```rust
//! Network toolbar with URL filter and dropdown filters.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network_filter::{MethodFilter, ProtocolFilter, StatusFilter};
use super::super::{MANTLE, SURFACE0, SURFACE1, OVERLAY0, TEXT, BLUE, GREEN, YELLOW, PEACH, RED};

pub fn draw_network_toolbar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let mut spans: Vec<Span> = Vec::new();
    let mut x: u16 = 0;

    // URL filter
    let url_active = false; // TODO: wire to AppMode when network search is implemented
    let si = if url_active {
        Style::default().fg(MANTLE).bg(YELLOW)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled(" /", si));
    x += 2;

    let sw: usize = 20;
    let st = if app.network.filter.url_query.is_empty() {
        "filter url...".to_string()
    } else {
        app.network.filter.url_query.clone()
    };
    let ss = if !app.network.filter.url_query.is_empty() {
        Style::default().fg(YELLOW).bg(bg)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    spans.push(Span::styled(crate::ui::safe_pad(&st, sw), ss));
    x += sw as u16;

    spans.push(Span::styled("   ", Style::default().bg(bg)));
    x += 3;

    // Protocol dropdown
    let proto_label = match app.network.filter.protocol {
        ProtocolFilter::All => "Protocol ▼",
        ProtocolFilter::Http => "HTTP ▼",
        ProtocolFilter::Sse => "SSE ▼",
        ProtocolFilter::Ws => "WS ▼",
    };
    let proto_style = if app.network.filter.protocol != ProtocolFilter::All {
        Style::default().fg(MANTLE).bg(BLUE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    let proto_start = x;
    spans.push(Span::styled(format!(" {} ", proto_label), proto_style));
    x += proto_label.width() as u16 + 2;

    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Method dropdown
    let method_label = match app.network.filter.method {
        MethodFilter::All => "Method ▼",
        MethodFilter::Get => "GET ▼",
        MethodFilter::Post => "POST ▼",
        MethodFilter::Put => "PUT ▼",
        MethodFilter::Delete => "DELETE ▼",
        MethodFilter::Patch => "PATCH ▼",
    };
    let method_style = if app.network.filter.method != MethodFilter::All {
        Style::default().fg(MANTLE).bg(GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    let method_start = x;
    spans.push(Span::styled(format!(" {} ", method_label), method_style));
    x += method_label.width() as u16 + 2;

    spans.push(Span::styled("  ", Style::default().bg(bg)));
    x += 2;

    // Status dropdown
    let status_label = match app.network.filter.status {
        StatusFilter::All => "Status ▼",
        StatusFilter::Success2xx => "2xx ▼",
        StatusFilter::Redirect3xx => "3xx ▼",
        StatusFilter::ClientError4xx => "4xx ▼",
        StatusFilter::ServerError5xx => "5xx ▼",
        StatusFilter::Failed => "Failed ▼",
    };
    let status_style = if app.network.filter.status != StatusFilter::All {
        Style::default().fg(MANTLE).bg(YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(OVERLAY0).bg(bg)
    };
    let status_start = x;
    spans.push(Span::styled(format!(" {} ", status_label), status_style));
    x += status_label.width() as u16 + 2;

    // Fill remaining
    let rem = area.width.saturating_sub(x);
    if rem > 0 {
        spans.push(Span::styled(" ".repeat(rem as usize), Style::default().bg(bg)));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles (may have warnings about unused imports until wired up).

- [ ] **Step 3: Commit**

```bash
git add src/ui/network/filter.rs
git commit -m "feat(ui/network): add toolbar with URL/protocol/method/status filters"
```

---

### Task 2: Network Request Table Renderer

**Files:**
- Modify: `src/ui/network/mod.rs`

- [ ] **Step 1: Replace placeholder with full network view**

Replace the placeholder `draw_network()` in `src/ui/network/mod.rs` with the full implementation:

```rust
//! Network Inspector view.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use unicode_width::UnicodeWidthStr;

pub mod detail;
pub mod filter;

use crate::app::App;
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol};
use super::{
    BASE, MANTLE, SURFACE0, SURFACE1, OVERLAY0, TEXT, SUBTEXT0,
    BLUE, SAPPHIRE, TEAL, GREEN, YELLOW, PEACH, RED, MAUVE, PINK, LAVENDER,
    safe_pad, safe_truncate,
};

const ERROR_ROW_BG: Color   = Color::Rgb(50, 30, 35);
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);

// Column widths
const PROTO_W: usize = 6;
const METHOD_W: usize = 8;
const STATUS_W: usize = 10;
const TIME_W: usize = 8;
const SIZE_W: usize = 8;

fn method_color(method: &str) -> Color {
    match method.to_uppercase().as_str() {
        "GET" => GREEN,
        "POST" => BLUE,
        "PUT" => PEACH,
        "DELETE" => RED,
        "PATCH" => MAUVE,
        _ => SUBTEXT0,
    }
}

fn status_color(status: Option<u16>) -> (Color, bool) {
    match status {
        Some(s) if s >= 500 => (RED, true),
        Some(s) if s >= 400 => (YELLOW, true),
        Some(s) if s >= 300 => (BLUE, false),
        Some(s) if s >= 200 => (GREEN, false),
        _ => (OVERLAY0, false),
    }
}

fn duration_color(ms: Option<u64>) -> Color {
    match ms {
        Some(d) if d > 1000 => RED,
        Some(d) if d > 200 => PEACH,
        _ => SUBTEXT0,
    }
}

fn format_duration(ms: Option<u64>) -> String {
    match ms {
        None => "—".to_string(),
        Some(d) if d >= 1000 => format!("{:.1}s", d as f64 / 1000.0),
        Some(d) => format!("{}ms", d),
    }
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        "—".to_string()
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}K", bytes as f64 / 1024.0)
    } else {
        format!("{}B", bytes)
    }
}

pub fn draw_network(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // toolbar
            Constraint::Length(1),  // table header
            Constraint::Min(1),    // table body
            Constraint::Length(1),  // status bar
        ])
        .split(area);

    filter::draw_network_toolbar(f, app, rows[0]);
    draw_table_header(f, rows[1]);

    // Main area: table + optional detail panel
    if app.network.show_detail {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(rows[2]);
        draw_table_body(f, app, cols[0]);
        detail::draw_network_detail(f, app, cols[1]);
    } else {
        draw_table_body(f, app, rows[2]);
    }

    draw_network_status_bar(f, app, rows[3]);
}

fn draw_table_header(f: &mut Frame, area: Rect) {
    let bg = MANTLE;
    let total_w = area.width as usize;
    let url_w = total_w.saturating_sub(PROTO_W + METHOD_W + STATUS_W + TIME_W + SIZE_W + 6);

    let spans = vec![
        Span::styled(safe_pad("PROTO", PROTO_W), Style::default().fg(OVERLAY0).bg(bg).add_modifier(Modifier::BOLD)),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("METHOD", METHOD_W), Style::default().fg(OVERLAY0).bg(bg).add_modifier(Modifier::BOLD)),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("URL", url_w), Style::default().fg(OVERLAY0).bg(bg).add_modifier(Modifier::BOLD)),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("STATUS", STATUS_W), Style::default().fg(OVERLAY0).bg(bg).add_modifier(Modifier::BOLD)),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("TIME", TIME_W), Style::default().fg(OVERLAY0).bg(bg).add_modifier(Modifier::BOLD)),
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled(safe_pad("SIZE", SIZE_W), Style::default().fg(OVERLAY0).bg(bg).add_modifier(Modifier::BOLD)),
    ];

    f.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().bg(bg).add_modifier(Modifier::UNDERLINED)),
        area,
    );
}

fn draw_table_body(f: &mut Frame, app: &mut App, area: Rect) {
    let height = area.height as usize;
    let total_w = area.width as usize;
    let url_w = total_w.saturating_sub(PROTO_W + METHOD_W + STATUS_W + TIME_W + SIZE_W + 6);

    let fi_vec: Vec<usize> = app.network.filtered_indices(&app.network_store).to_vec();
    let filtered_count = fi_vec.len();

    if filtered_count == 0 {
        draw_empty_network(f, app, area);
        return;
    }

    // Clamp selection
    app.network.selected = app.network.selected.min(filtered_count.saturating_sub(1));
    app.network.scroll_offset = app.network.scroll_offset.min(filtered_count.saturating_sub(1));

    // Ensure selected is visible
    if app.network.selected < app.network.scroll_offset {
        app.network.scroll_offset = app.network.selected;
    }
    if app.network.selected >= app.network.scroll_offset + height {
        app.network.scroll_offset = app.network.selected.saturating_sub(height - 1);
    }

    let start = app.network.scroll_offset;
    let end = (start + height).min(filtered_count);

    let mut lines: Vec<Line> = Vec::new();

    for vi in start..end {
        let store_idx = fi_vec[vi];
        let is_selected = vi == app.network.selected;

        if let Some(entry) = app.network_store.get(store_idx) {
            let row_bg = if is_selected {
                SURFACE1
            } else {
                match entry.http_status {
                    Some(s) if s >= 500 => ERROR_ROW_BG,
                    Some(s) if s >= 400 => WARNING_ROW_BG,
                    _ if entry.status == NetworkStatus::Failed => ERROR_ROW_BG,
                    _ => BASE,
                }
            };

            let cursor = if is_selected {
                Span::styled("▎", Style::default().fg(BLUE).bg(row_bg))
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            };

            // Protocol pill
            let proto_style = match entry.protocol {
                Protocol::Http => Style::default().fg(SUBTEXT0).bg(row_bg),
                Protocol::Sse => Style::default().fg(MANTLE).bg(PEACH).add_modifier(Modifier::BOLD),
                Protocol::Ws => Style::default().fg(MANTLE).bg(SAPPHIRE).add_modifier(Modifier::BOLD),
            };
            let proto_text = safe_pad(entry.protocol.as_str(), PROTO_W - 1);

            // Method pill
            let mc = method_color(&entry.method);
            let method_text = if entry.method.is_empty() {
                safe_pad("──", METHOD_W)
            } else {
                safe_pad(&entry.method, METHOD_W)
            };
            let method_style = if entry.method.is_empty() {
                Style::default().fg(OVERLAY0).bg(row_bg)
            } else {
                Style::default().fg(MANTLE).bg(mc).add_modifier(Modifier::BOLD)
            };

            // URL
            let url_text = safe_truncate(&entry.path, url_w);
            let url_padded = safe_pad(&url_text, url_w);

            // Status
            let status_text = match entry.status {
                NetworkStatus::Pending => safe_pad("⏳", STATUS_W),
                NetworkStatus::Active => {
                    let dot = match (app.tick / 8) % 4 { 0 => "●", 1 => "◉", 2 => "●", _ => "○" };
                    let label = match entry.protocol {
                        Protocol::Sse => format!("{} stream", dot),
                        Protocol::Ws => format!("{} live", dot),
                        _ => format!("{}", dot),
                    };
                    safe_pad(&label, STATUS_W)
                }
                NetworkStatus::Completed => {
                    let s = entry.http_status.map_or("OK".to_string(), |s| s.to_string());
                    safe_pad(&s, STATUS_W)
                }
                NetworkStatus::Failed => safe_pad("FAIL", STATUS_W),
            };
            let (sc, sb) = status_color(entry.http_status);
            let status_style = if entry.status == NetworkStatus::Failed {
                Style::default().fg(RED).bg(row_bg).add_modifier(Modifier::BOLD)
            } else if entry.status == NetworkStatus::Active {
                Style::default().fg(GREEN).bg(row_bg)
            } else if sb {
                Style::default().fg(sc).bg(row_bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(sc).bg(row_bg)
            };

            // Duration
            let dur_text = safe_pad(&format_duration(entry.duration), TIME_W);
            let dur_style = Style::default().fg(duration_color(entry.duration)).bg(row_bg);

            // Size
            let size_text = safe_pad(&format_size(entry.display_size()), SIZE_W);

            let mut spans = vec![
                cursor,
                Span::styled(proto_text, proto_style),
                Span::styled(" ", Style::default().bg(row_bg)),
                Span::styled(method_text, method_style),
                Span::styled(" ", Style::default().bg(row_bg)),
                Span::styled(url_padded, Style::default().fg(TEXT).bg(row_bg)),
                Span::styled(" ", Style::default().bg(row_bg)),
                Span::styled(status_text, status_style),
                Span::styled(" ", Style::default().bg(row_bg)),
                Span::styled(dur_text, dur_style),
                Span::styled(" ", Style::default().bg(row_bg)),
                Span::styled(size_text, Style::default().fg(SUBTEXT0).bg(row_bg)),
            ];

            // Pad to full width & add underline
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            if used < total_w {
                spans.push(Span::styled(
                    " ".repeat(total_w - used),
                    Style::default().bg(row_bg),
                ));
            }

            // Underline separator
            for span in spans.iter_mut() {
                span.style = span.style.add_modifier(Modifier::UNDERLINED);
            }

            lines.push(Line::from(spans));
        }
    }

    // Fill remaining height
    while lines.len() < height {
        lines.push(Line::from(Span::styled(
            " ".repeat(total_w),
            Style::default().bg(BASE),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(BASE)),
        area,
    );

    // Scrollbar
    if filtered_count > height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("┃")
            .track_symbol(Some(" "))
            .thumb_style(Style::default().fg(BLUE))
            .track_style(Style::default().fg(SURFACE0).bg(BASE))
            .begin_symbol(None)
            .end_symbol(None);
        let max_offset = filtered_count.saturating_sub(height);
        let mut state = ScrollbarState::new(max_offset)
            .position(app.network.scroll_offset.min(max_offset));
        f.render_stateful_widget(scrollbar, area, &mut state);
    }
}

fn draw_empty_network(f: &mut Frame, app: &App, area: Rect) {
    let mid_y = area.height / 2;
    let mut lines: Vec<Line> = Vec::new();

    for _ in 0..mid_y.saturating_sub(3) {
        lines.push(Line::raw(""));
    }

    if app.network_store.is_empty() {
        lines.push(Line::from(Span::styled(
            "    Network Inspector",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    Add FlogHttpInterceptor to your Dio instance",
            Style::default().fg(OVERLAY0),
        )));
        lines.push(Line::from(Span::styled(
            "    to see network requests here.",
            Style::default().fg(SURFACE1),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "    No matching requests",
            Style::default().fg(OVERLAY0),
        )));
        lines.push(Line::from(Span::styled(
            "    Try adjusting your filters",
            Style::default().fg(SURFACE1),
        )));
    }

    f.render_widget(
        Paragraph::new(lines).style(Style::default().bg(BASE)),
        area,
    );
}

fn draw_network_status_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let bg = MANTLE;
    let fi_vec = app.network.filtered_indices(&app.network_store);
    let total = app.network_store.len();
    let filtered = fi_vec.len();
    let failed = app.network_store.iter()
        .filter(|e| e.status == NetworkStatus::Failed || e.http_status.map_or(false, |s| s >= 400))
        .count();

    let info = format!(" {} requests", total);
    let fail_info = if failed > 0 {
        format!("  {} failed", failed)
    } else {
        String::new()
    };

    let buttons: Vec<(&str, &str, Style)> = vec![
        ("separator", " ── ", Style::default().fg(YELLOW).bg(bg)),
        ("clear", " Clear ", Style::default().fg(PEACH).bg(bg)),
        ("export", " Export ", Style::default().fg(SAPPHIRE).bg(bg)),
        ("help", " ? ", Style::default().fg(SAPPHIRE).bg(bg)),
        ("quit", " x ", Style::default().fg(RED).bg(bg)),
    ];

    let bw: u16 = buttons.iter().map(|(_, l, _)| l.width() as u16 + 1).sum();
    let iw = info.width() as u16 + fail_info.width() as u16;
    let spacer = area.width.saturating_sub(iw + bw).max(1);

    let mut spans = vec![
        Span::styled(&info, Style::default().fg(SUBTEXT0).bg(bg)),
    ];

    if failed > 0 {
        spans.push(Span::styled(&fail_info, Style::default().fg(RED).bg(bg)));
    }

    spans.push(Span::styled(" ".repeat(spacer as usize), Style::default().bg(bg)));

    for (_, label, style) in &buttons {
        spans.push(Span::styled(*label, *style));
        spans.push(Span::styled(" ", Style::default().bg(bg)));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1 | head -30`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add src/ui/network/mod.rs
git commit -m "feat(ui/network): full request table with protocol/method pills, status colors, scrollbar"
```

---

### Task 3: Network Detail Panel

**Files:**
- Create: `src/ui/network/detail.rs`

- [ ] **Step 1: Create `src/ui/network/detail.rs`**

```rust
//! Network request detail panel with collapsible sections.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::domain::network::{NetworkEntry, NetworkStatus, Protocol, WsDirection};
use super::super::{
    MANTLE, SURFACE0, SURFACE1, OVERLAY0, TEXT, SUBTEXT0,
    BLUE, SAPPHIRE, TEAL, GREEN, YELLOW, PEACH, RED, MAUVE, PINK,
};
use super::{format_duration, format_size, method_color, status_color};

pub fn draw_network_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let fi_vec: Vec<usize> = app.network.filtered_indices(&app.network_store).to_vec();
    let store_idx = fi_vec.get(app.network.selected).copied();

    let entry = match store_idx.and_then(|idx| app.network_store.get(idx)) {
        Some(e) => e.clone(),
        None => {
            let block = Block::default()
                .title(" Details ")
                .borders(Borders::LEFT)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(SURFACE0))
                .style(Style::default().bg(MANTLE));
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  Select a request",
                    Style::default().fg(OVERLAY0),
                )))
                .block(block),
                area,
            );
            return;
        }
    };

    let inner_w = area.width.saturating_sub(2) as usize;
    let inner_h = area.height.saturating_sub(2) as usize;
    let mut all_lines: Vec<Line> = Vec::new();

    // ── Header ──
    let mc = method_color(&entry.method);
    if !entry.method.is_empty() {
        all_lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", entry.method),
                Style::default().fg(MANTLE).bg(mc).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {}", entry.path), Style::default().fg(TEXT)),
        ]));
    } else {
        all_lines.push(Line::from(Span::styled(
            format!(" {}", entry.url),
            Style::default().fg(TEXT),
        )));
    }
    all_lines.push(Line::from(Span::styled(
        "─".repeat(inner_w),
        Style::default().fg(SURFACE0),
    )));

    // ── General section ──
    all_lines.push(Line::from(Span::styled(
        " ▼ General",
        Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
    )));

    let kv_style = Style::default().fg(MAUVE);
    let val_style = Style::default().fg(TEXT);

    all_lines.push(Line::from(vec![
        Span::styled("   URL: ", kv_style),
        Span::styled(
            crate::ui::safe_truncate(&entry.url, inner_w.saturating_sub(8)),
            val_style,
        ),
    ]));

    if !entry.method.is_empty() {
        all_lines.push(Line::from(vec![
            Span::styled("   Method: ", kv_style),
            Span::styled(&entry.method, val_style),
        ]));
    }

    // Status
    let status_text = match entry.status {
        NetworkStatus::Pending => "Pending...".to_string(),
        NetworkStatus::Active => match entry.protocol {
            Protocol::Sse => format!("Streaming ({} chunks)", entry.sse_chunks.len()),
            Protocol::Ws => format!("Connected ({} messages)", entry.ws_messages.len()),
            _ => "Active".to_string(),
        },
        NetworkStatus::Completed => entry
            .http_status
            .map_or("OK".to_string(), |s| format!("{}", s)),
        NetworkStatus::Failed => entry.error.clone().unwrap_or_else(|| "Failed".to_string()),
    };
    let (sc, _) = status_color(entry.http_status);
    all_lines.push(Line::from(vec![
        Span::styled("   Status: ", kv_style),
        Span::styled(
            &status_text,
            Style::default().fg(if entry.status == NetworkStatus::Failed { RED } else { sc }),
        ),
    ]));

    if let Some(d) = entry.duration {
        all_lines.push(Line::from(vec![
            Span::styled("   Duration: ", kv_style),
            Span::styled(format_duration(Some(d)), val_style),
        ]));
    }

    let size = entry.display_size();
    if size > 0 {
        all_lines.push(Line::from(vec![
            Span::styled("   Size: ", kv_style),
            Span::styled(format_size(size), val_style),
        ]));
    }

    all_lines.push(Line::from(Span::raw("")));

    // ── Request Headers ──
    if let Some(ref headers) = entry.request_headers {
        all_lines.push(Line::from(Span::styled(
            " ▼ Request Headers",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        )));
        render_json_compact(&mut all_lines, headers, inner_w, kv_style, val_style);
        all_lines.push(Line::from(Span::raw("")));
    }

    // ── Request Body ──
    if let Some(ref body) = entry.request_body {
        if !body.is_empty() {
            all_lines.push(Line::from(Span::styled(
                " ▼ Request Body",
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            )));
            render_body_lines(&mut all_lines, body, inner_w, val_style);
            all_lines.push(Line::from(Span::raw("")));
        }
    }

    // ── Response Headers ──
    if let Some(ref headers) = entry.response_headers {
        all_lines.push(Line::from(Span::styled(
            " ▼ Response Headers",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        )));
        render_json_compact(&mut all_lines, headers, inner_w, kv_style, val_style);
        all_lines.push(Line::from(Span::raw("")));
    }

    // ── Response Body ──
    if let Some(ref body) = entry.response_body {
        if !body.is_empty() {
            all_lines.push(Line::from(Span::styled(
                " ▼ Response Body",
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            )));
            render_body_lines(&mut all_lines, body, inner_w, val_style);
            all_lines.push(Line::from(Span::raw("")));
        }
    }

    // ── SSE Stream Events ──
    if !entry.sse_chunks.is_empty() {
        all_lines.push(Line::from(Span::styled(
            format!(
                " ▼ Stream Events ({} chunks, {} total)",
                entry.sse_chunks.len(),
                format_size(entry.sse_total_size)
            ),
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        )));

        let show_count = entry.sse_chunks.len().min(20);
        let start = entry.sse_chunks.len().saturating_sub(show_count);
        for chunk in &entry.sse_chunks[start..] {
            all_lines.push(Line::from(vec![
                Span::styled(format!("   #{:<4}", chunk.seq), Style::default().fg(OVERLAY0)),
                Span::styled(
                    crate::ui::safe_truncate(&chunk.data, inner_w.saturating_sub(10)),
                    Style::default().fg(GREEN),
                ),
            ]));
        }
        if entry.sse_chunks.len() > show_count {
            all_lines.push(Line::from(Span::styled(
                format!(
                    "   ── showing {} of {} ──",
                    show_count,
                    entry.sse_chunks.len()
                ),
                Style::default().fg(OVERLAY0),
            )));
        }
        all_lines.push(Line::from(Span::raw("")));
    }

    // ── WebSocket Messages ──
    if !entry.ws_messages.is_empty() {
        let sent = entry.ws_messages.iter().filter(|m| m.direction == WsDirection::Send).count();
        let recv = entry.ws_messages.iter().filter(|m| m.direction == WsDirection::Recv).count();
        all_lines.push(Line::from(Span::styled(
            format!(" ▼ Messages ({} sent / {} received)", sent, recv),
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        )));

        let show_count = entry.ws_messages.len().min(20);
        let start = entry.ws_messages.len().saturating_sub(show_count);
        for msg in entry.ws_messages[start..].iter().rev() {
            let (arrow, color) = match msg.direction {
                WsDirection::Send => ("→ send", GREEN),
                WsDirection::Recv => ("← recv", BLUE),
            };
            let data_display = if msg.data.starts_with("<binary") {
                msg.data.clone()
            } else {
                crate::ui::safe_truncate(&msg.data, inner_w.saturating_sub(20))
            };
            all_lines.push(Line::from(vec![
                Span::styled(format!("   {} ", arrow), Style::default().fg(color)),
                Span::styled(
                    data_display,
                    Style::default().fg(SUBTEXT0),
                ),
                Span::styled(
                    format!("  {}", format_size(msg.size)),
                    Style::default().fg(OVERLAY0),
                ),
            ]));
        }
        if entry.ws_messages.len() > show_count {
            all_lines.push(Line::from(Span::styled(
                format!(
                    "   ── showing {} of {} ──",
                    show_count,
                    entry.ws_messages.len()
                ),
                Style::default().fg(OVERLAY0),
            )));
        }
    }

    // ── Error ──
    if let Some(ref err) = entry.error {
        all_lines.push(Line::from(Span::styled(
            " ▼ Error",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )));
        all_lines.push(Line::from(Span::styled(
            format!("   {}", err),
            Style::default().fg(RED),
        )));
    }

    // Apply scroll
    let scroll = app.network.detail_scroll;
    let visible_lines: Vec<Line> = all_lines
        .into_iter()
        .skip(scroll)
        .take(inner_h)
        .collect();

    let block = Block::default()
        .title(" Details ")
        .title_style(Style::default().fg(BLUE).add_modifier(Modifier::BOLD))
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE0))
        .style(Style::default().bg(MANTLE));

    f.render_widget(
        Paragraph::new(visible_lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

/// Render JSON string as compact key-value lines.
fn render_json_compact(
    lines: &mut Vec<Line<'static>>,
    json_str: &str,
    max_w: usize,
    key_style: Style,
    val_style: Style,
) {
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(json_str) {
        for (key, value) in &map {
            let val_text = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("   {}: ", key), key_style),
                Span::styled(
                    crate::ui::safe_truncate(&val_text, max_w.saturating_sub(key.len() + 6)),
                    val_style,
                ),
            ]));
        }
    } else {
        // Not valid JSON, render as raw text
        for line in json_str.lines().take(10) {
            lines.push(Line::from(Span::styled(
                format!("   {}", crate::ui::safe_truncate(line, max_w.saturating_sub(3))),
                val_style,
            )));
        }
    }
}

/// Render body content with basic formatting.
fn render_body_lines(
    lines: &mut Vec<Line<'static>>,
    body: &str,
    max_w: usize,
    val_style: Style,
) {
    // Try pretty-print JSON
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(body) {
        if let Ok(pretty) = serde_json::to_string_pretty(&parsed) {
            for line in pretty.lines().take(30) {
                lines.push(Line::from(Span::styled(
                    format!("   {}", crate::ui::safe_truncate(line, max_w.saturating_sub(3))),
                    val_style,
                )));
            }
            return;
        }
    }

    // Raw text
    for line in body.lines().take(20) {
        lines.push(Line::from(Span::styled(
            format!("   {}", crate::ui::safe_truncate(line, max_w.saturating_sub(3))),
            val_style,
        )));
    }
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1 | head -30`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add src/ui/network/detail.rs
git commit -m "feat(ui/network): detail panel with General/Headers/Body/SSE/WS sections"
```

---

### Task 4: Network Event Handling (Keyboard + Mouse)

**Files:**
- Modify: `src/event.rs`

- [ ] **Step 1: Add network keyboard handling**

In `handle_key()`, when `app.active_tab == ViewTab::Network` and `app.mode == AppMode::Normal`, add:

```rust
// Network view keys (Normal mode)
if app.active_tab == ViewTab::Network {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let count = app.network.filtered_count(&app.network_store);
            if app.network.selected + 1 < count {
                app.network.selected += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.network.selected > 0 {
                app.network.selected -= 1;
            }
        }
        KeyCode::Enter => {
            app.network.show_detail = !app.network.show_detail;
        }
        KeyCode::PageDown => {
            let count = app.network.filtered_count(&app.network_store);
            app.network.selected = (app.network.selected + 20).min(count.saturating_sub(1));
        }
        KeyCode::PageUp => {
            app.network.selected = app.network.selected.saturating_sub(20);
        }
        KeyCode::Home => {
            app.network.selected = 0;
            app.network.scroll_offset = 0;
        }
        KeyCode::End => {
            let count = app.network.filtered_count(&app.network_store);
            app.network.selected = count.saturating_sub(1);
        }
        KeyCode::Esc => {
            app.network.filter.clear();
            app.network.invalidate_filter();
        }
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('?') => app.mode = AppMode::Help,
        _ => {}
    }
    return;  // Don't fall through to Logs handling
}
```

- [ ] **Step 2: Add network mouse handling**

In `handle_mouse()`, when `app.active_tab == ViewTab::Network`, add handling for:
- Scroll up/down → navigate list
- Left click on row → select + toggle detail
- Scroll in detail panel → scroll detail

```rust
// Network view mouse (after tab bar check)
if app.active_tab == ViewTab::Network {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            let count = app.network.filtered_count(&app.network_store);
            if app.network.selected + 3 < count {
                app.network.selected += 3;
            }
        }
        MouseEventKind::ScrollUp => {
            app.network.selected = app.network.selected.saturating_sub(3);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // Calculate which row was clicked based on area
            // Row index = mouse.row - content_area_y - header_row(1) + scroll_offset
            // For simplicity, use the toolbar(1) + header(1) offset from tab bar
            let content_start_y = app.layout.tab_bar_y + 1 + 1 + 1; // tab + toolbar + header
            if mouse.row >= content_start_y {
                let clicked_row = (mouse.row - content_start_y) as usize;
                let new_selected = app.network.scroll_offset + clicked_row;
                let count = app.network.filtered_count(&app.network_store);
                if new_selected < count {
                    if app.network.selected == new_selected {
                        app.network.show_detail = !app.network.show_detail;
                    } else {
                        app.network.selected = new_selected;
                        app.network.show_detail = true;
                    }
                }
            }
        }
        _ => {}
    }
    return;
}
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/event.rs
git commit -m "feat(event): add keyboard and mouse handling for Network view"
```

---

### Task 5: Dart FlogHttpInterceptor

**Files:**
- Create: `flog_logger/lib/src/flog_net.dart`
- Create: `flog_logger/lib/src/flog_http_interceptor.dart`
- Modify: `flog_logger/lib/flog_logger.dart`
- Modify: `flog_logger/pubspec.yaml`

- [ ] **Step 1: Add dio dependency to pubspec.yaml**

```yaml
name: flog_logger
description: Lightweight structured logger for Flutter. Outputs [LEVEL][Tag] format that flog parses natively.
version: 0.2.0
homepage: https://github.com/shaomingqing/flog
repository: https://github.com/shaomingqing/flog/tree/master/flog_logger

environment:
  sdk: ^3.0.0

dependencies:
  dio: ">=4.0.0 <6.0.0"
  web_socket_channel: ">=2.0.0 <4.0.0"
```

- [ ] **Step 2: Create `flog_logger/lib/src/flog_net.dart`**

Shared helper for network logging:

```dart
/// Internal helper for flog_net protocol.
library;

import 'dart:convert';

const _tag = 'flog_net';

int _nextId = 1;

/// Get next unique request ID.
int nextNetId() => _nextId++;

/// Emit a flog_net protocol message.
void emitNet(Map<String, dynamic> data) {
  // ignore: avoid_print
  print('[INFO][$_tag] ${jsonEncode(data)}');
}
```

- [ ] **Step 3: Create `flog_logger/lib/src/flog_http_interceptor.dart`**

```dart
/// HTTP interceptor for Dio that emits flog_net protocol messages.
library;

import 'dart:convert';
import 'package:dio/dio.dart';
import 'flog_net.dart';

/// Interceptor that logs HTTP requests to flog's Network Inspector.
///
/// ```dart
/// final dio = Dio();
/// dio.interceptors.add(FlogHttpInterceptor());
/// ```
class FlogHttpInterceptor extends Interceptor {
  /// Whether to include request headers in the log.
  final bool includeRequestHeaders;

  /// Whether to include response headers in the log.
  final bool includeResponseHeaders;

  /// Whether to include request body in the log.
  final bool includeRequestBody;

  /// Whether to include response body in the log.
  final bool includeResponseBody;

  /// Maximum body size in bytes before truncation.
  final int maxBodySize;

  /// Optional filter — return false to skip logging this request.
  final bool Function(RequestOptions)? filter;

  /// Map from request hashCode to assigned flog_net id.
  final Map<int, int> _requestIds = {};

  /// Map from request hashCode to start time.
  final Map<int, DateTime> _startTimes = {};

  FlogHttpInterceptor({
    this.includeRequestHeaders = true,
    this.includeResponseHeaders = true,
    this.includeRequestBody = true,
    this.includeResponseBody = true,
    this.maxBodySize = 10 * 1024,
    this.filter,
  });

  @override
  void onRequest(RequestOptions options, RequestInterceptorHandler handler) {
    if (filter != null && !filter!(options)) {
      handler.next(options);
      return;
    }

    final id = nextNetId();
    _requestIds[options.hashCode] = id;
    _startTimes[options.hashCode] = DateTime.now();

    final data = <String, dynamic>{
      'id': id,
      't': 'req',
      'p': 'http',
      'method': options.method,
      'url': options.uri.toString(),
    };

    if (includeRequestHeaders && options.headers.isNotEmpty) {
      data['headers'] = options.headers;
    }

    if (includeRequestBody && options.data != null) {
      data['body'] = _truncateBody(options.data);
    }

    emitNet(data);
    handler.next(options);
  }

  @override
  void onResponse(Response response, ResponseInterceptorHandler handler) {
    final reqHash = response.requestOptions.hashCode;
    final id = _requestIds.remove(reqHash);
    final startTime = _startTimes.remove(reqHash);

    if (id == null) {
      handler.next(response);
      return;
    }

    final duration = startTime != null
        ? DateTime.now().difference(startTime).inMilliseconds
        : null;

    final data = <String, dynamic>{
      'id': id,
      't': 'res',
      'p': 'http',
      'status': response.statusCode,
    };

    if (duration != null) data['duration'] = duration;

    if (includeResponseHeaders && response.headers.map.isNotEmpty) {
      data['headers'] = response.headers.map;
    }

    if (includeResponseBody && response.data != null) {
      final body = _truncateBody(response.data);
      data['body'] = body;
      data['size'] = _estimateSize(body);
    }

    emitNet(data);
    handler.next(response);
  }

  @override
  void onError(DioException err, ErrorInterceptorHandler handler) {
    final reqHash = err.requestOptions.hashCode;
    final id = _requestIds.remove(reqHash);
    final startTime = _startTimes.remove(reqHash);

    if (id != null) {
      final duration = startTime != null
          ? DateTime.now().difference(startTime).inMilliseconds
          : null;

      final data = <String, dynamic>{
        'id': id,
        't': 'err',
        'p': 'http',
        'error': err.message ?? err.type.toString(),
      };

      if (duration != null) data['duration'] = duration;
      if (err.response?.statusCode != null) {
        data['status'] = err.response!.statusCode;
      }

      emitNet(data);
    }

    handler.next(err);
  }

  String _truncateBody(dynamic body) {
    String text;
    if (body is String) {
      text = body;
    } else if (body is Map || body is List) {
      text = jsonEncode(body);
    } else {
      text = body.toString();
    }
    if (text.length > maxBodySize) {
      return '${text.substring(0, maxBodySize)}... (truncated)';
    }
    return text;
  }

  int _estimateSize(dynamic data) {
    if (data is String) return data.length;
    return data.toString().length;
  }
}
```

- [ ] **Step 4: Update `flog_logger/lib/flog_logger.dart` to export new classes**

Add at the end of the file:

```dart
export 'src/flog_net.dart' show nextNetId, emitNet;
export 'src/flog_http_interceptor.dart';
```

- [ ] **Step 5: Commit**

```bash
git add flog_logger/
git commit -m "feat(flog_logger): add FlogHttpInterceptor for dio"
```

---

### Task 6: Dart FlogSseParser

**Files:**
- Create: `flog_logger/lib/src/flog_sse_parser.dart`
- Modify: `flog_logger/lib/flog_logger.dart`

- [ ] **Step 1: Create `flog_logger/lib/src/flog_sse_parser.dart`**

```dart
/// SSE parser wrapper that emits flog_net protocol messages.
library;

import 'dart:convert';
import 'flog_net.dart';

/// A single SSE event.
class SseEvent {
  final Map<String, dynamic> data;
  const SseEvent({required this.data});
}

/// Wraps an SSE byte stream, parsing events and logging to flog's Network Inspector.
///
/// ```dart
/// final response = await dio.post<ResponseBody>(url,
///   options: Options(responseType: ResponseType.stream));
/// await for (final event in FlogSseParser.parse(
///   response.data!.stream,
///   url: url,
///   method: 'POST',
/// )) {
///   // handle event
/// }
/// ```
class FlogSseParser {
  static Stream<SseEvent> parse(
    Stream<List<int>> byteStream, {
    required String url,
    required String method,
    Map<String, dynamic>? headers,
    String? requestBody,
  }) async* {
    final id = nextNetId();
    final startTime = DateTime.now();
    int seq = 0;
    int totalSize = 0;

    // Emit request
    final reqData = <String, dynamic>{
      'id': id,
      't': 'req',
      'p': 'sse',
      'method': method,
      'url': url,
    };
    if (headers != null) reqData['headers'] = headers;
    if (requestBody != null) reqData['body'] = requestBody;
    emitNet(reqData);

    final pendingBytes = <int>[];
    String buffer = '';

    try {
      await for (final chunk in byteStream) {
        pendingBytes.addAll(chunk);

        late String decoded;
        try {
          decoded = utf8.decode(pendingBytes);
          pendingBytes.clear();
        } catch (_) {
          continue;
        }

        buffer += decoded;
        final lines = buffer.split('\n');
        buffer = lines.removeLast();

        for (final line in lines) {
          if (!line.startsWith('data:')) continue;

          final dataStr = line.substring(5).trim();
          if (dataStr.isEmpty) continue;
          if (dataStr == '[DONE]') {
            // Emit done
            final duration =
                DateTime.now().difference(startTime).inMilliseconds;
            emitNet({
              'id': id,
              't': 'done',
              'p': 'sse',
              'duration': duration,
              'chunks': seq,
              'size': totalSize,
            });
            return;
          }

          try {
            final json = jsonDecode(dataStr) as Map<String, dynamic>;
            seq++;
            totalSize += dataStr.length;

            // Emit chunk
            emitNet({
              'id': id,
              't': 'chunk',
              'p': 'sse',
              'data': dataStr,
              'seq': seq,
              'size': dataStr.length,
            });

            yield SseEvent(data: json);
          } catch (e) {
            // Skip unparseable events
          }
        }
      }

      // Stream ended without [DONE]
      final duration = DateTime.now().difference(startTime).inMilliseconds;
      emitNet({
        'id': id,
        't': 'done',
        'p': 'sse',
        'duration': duration,
        'chunks': seq,
        'size': totalSize,
      });
    } catch (e) {
      final duration = DateTime.now().difference(startTime).inMilliseconds;
      emitNet({
        'id': id,
        't': 'err',
        'p': 'sse',
        'error': e.toString(),
        'duration': duration,
      });
      rethrow;
    }
  }
}
```

- [ ] **Step 2: Export from flog_logger.dart**

Add to `flog_logger/lib/flog_logger.dart`:

```dart
export 'src/flog_sse_parser.dart';
```

- [ ] **Step 3: Commit**

```bash
git add flog_logger/
git commit -m "feat(flog_logger): add FlogSseParser for SSE streaming"
```

---

### Task 7: Dart FlogWebSocket

**Files:**
- Create: `flog_logger/lib/src/flog_web_socket.dart`
- Modify: `flog_logger/lib/flog_logger.dart`

- [ ] **Step 1: Create `flog_logger/lib/src/flog_web_socket.dart`**

```dart
/// WebSocket wrapper that emits flog_net protocol messages.
library;

import 'dart:async';
import 'dart:convert';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'package:web_socket_channel/io.dart';
import 'flog_net.dart';

/// A WebSocket connection that logs all messages to flog's Network Inspector.
///
/// ```dart
/// final ws = await FlogWebSocket.connect('wss://example.com/ws');
/// ws.send(jsonEncode({'type': 'hello'}));
/// ws.stream.listen((data) => print(data));
/// await ws.close();
/// ```
class FlogWebSocket {
  final WebSocketChannel _channel;
  final int _id;
  final DateTime _startTime;
  final StreamController<dynamic> _controller =
      StreamController<dynamic>.broadcast();
  bool _closed = false;

  FlogWebSocket._(this._channel, this._id, this._startTime) {
    _channel.stream.listen(
      (data) {
        final size = _estimateSize(data);
        final display = _formatData(data);
        emitNet({
          'id': _id,
          't': 'recv',
          'p': 'ws',
          'data': display,
          'size': size,
        });
        _controller.add(data);
      },
      onError: (error) {
        emitNet({
          'id': _id,
          't': 'err',
          'p': 'ws',
          'error': error.toString(),
        });
        _controller.addError(error);
      },
      onDone: () {
        if (!_closed) {
          final duration =
              DateTime.now().difference(_startTime).inMilliseconds;
          emitNet({
            'id': _id,
            't': 'close',
            'p': 'ws',
            'code': _channel.closeCode,
            'reason': _channel.closeReason ?? 'Connection closed',
            'duration': duration,
          });
        }
        _controller.close();
      },
    );
  }

  /// Connect to a WebSocket endpoint.
  static Future<FlogWebSocket> connect(
    String url, {
    Map<String, dynamic>? headers,
  }) async {
    final id = nextNetId();
    final startTime = DateTime.now();

    emitNet({
      'id': id,
      't': 'open',
      'p': 'ws',
      'url': url,
    });

    final channel = IOWebSocketChannel.connect(
      Uri.parse(url),
      headers: headers,
    );
    await channel.ready;

    return FlogWebSocket._(channel, id, startTime);
  }

  /// Send data through the WebSocket.
  void send(dynamic data) {
    final size = _estimateSize(data);
    final display = _formatData(data);
    emitNet({
      'id': _id,
      't': 'send',
      'p': 'ws',
      'data': display,
      'size': size,
    });
    _channel.sink.add(data);
  }

  /// Stream of incoming messages.
  Stream<dynamic> get stream => _controller.stream;

  /// Close the WebSocket connection.
  Future<void> close([int? code, String? reason]) async {
    _closed = true;
    final duration = DateTime.now().difference(_startTime).inMilliseconds;
    emitNet({
      'id': _id,
      't': 'close',
      'p': 'ws',
      'code': code ?? 1000,
      'reason': reason ?? 'Normal closure',
      'duration': duration,
    });
    await _channel.sink.close(code, reason);
  }

  /// The underlying WebSocket channel's close code.
  int? get closeCode => _channel.closeCode;

  /// The underlying WebSocket channel's close reason.
  String? get closeReason => _channel.closeReason;

  int _estimateSize(dynamic data) {
    if (data is String) return data.length;
    if (data is List<int>) return data.length;
    return data.toString().length;
  }

  String _formatData(dynamic data) {
    if (data is String) return data;
    if (data is List<int>) return '<binary: ${data.length} bytes>';
    return data.toString();
  }
}
```

- [ ] **Step 2: Export from flog_logger.dart**

Add to `flog_logger/lib/flog_logger.dart`:

```dart
export 'src/flog_web_socket.dart';
```

- [ ] **Step 3: Commit**

```bash
git add flog_logger/
git commit -m "feat(flog_logger): add FlogWebSocket wrapper"
```

---

### Task 8: Create flog_logger lib/src Directory and Fix Exports

**Files:**
- Create: `flog_logger/lib/src/` directory
- Modify: `flog_logger/lib/flog_logger.dart`

- [ ] **Step 1: Ensure directory structure**

```bash
mkdir -p flog_logger/lib/src
```

- [ ] **Step 2: Verify final flog_logger.dart exports**

The final `flog_logger/lib/flog_logger.dart` should be:

```dart
/// Lightweight structured logger for Flutter.
///
/// Outputs `[LEVEL][Tag] message` format that
/// [flog](https://github.com/shaomingqing/flog) parses natively.
///
/// ```dart
/// final log = FlogLogger('Network');
/// log.i('-> GET /api/users');
/// log.e('Connection failed', error: e, stackTrace: st);
/// ```
library flog_logger;

// Core logger
class FlogLogger {
  // ... existing code unchanged ...
}

// Network interceptors
export 'src/flog_net.dart' show nextNetId, emitNet;
export 'src/flog_http_interceptor.dart';
export 'src/flog_sse_parser.dart';
export 'src/flog_web_socket.dart';
```

- [ ] **Step 3: Commit**

```bash
git add flog_logger/
git commit -m "chore(flog_logger): finalize exports and directory structure"
```

---

### Task 9: Final Build, Test, and Verification

**Files:** None (verification only)

- [ ] **Step 1: Run Rust test suite**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 2: Run Rust clippy**

Run: `cargo clippy 2>&1 | head -40`
Expected: No errors.

- [ ] **Step 3: Run Rust fmt**

Run: `cargo fmt -- --check 2>&1`
Expected: No formatting issues.

- [ ] **Step 4: Build Rust release**

Run: `cargo build --release 2>&1 | head -10`
Expected: Compiles successfully.

- [ ] **Step 5: Verify Dart package**

Run: `cd flog_logger && dart analyze 2>&1`
Expected: No errors (warnings about unused imports acceptable).

- [ ] **Step 6: Manual smoke test**

1. Run flog with a Flutter app that uses `FlogHttpInterceptor`
2. Verify Network tab shows requests with correct columns
3. Click a request → detail panel opens with headers/body
4. Test SSE requests appear with streaming status
5. Test WebSocket connections appear with send/recv messages
6. Test filters: Protocol, Method, Status dropdowns work
7. Test keyboard navigation: j/k, Enter, Esc, 1/2 tab switch
