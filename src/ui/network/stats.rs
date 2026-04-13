//! Network Statistics panel — full-screen overlay with latency percentiles,
//! top-5 slowest requests, status code distribution, and per-domain breakdown.

use std::collections::HashMap;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::app::App;
use crate::domain::network::NetworkStatus;

// Catppuccin Macchiato palette
const TEXT: Color = Color::Rgb(202, 211, 245);
const SUBTEXT0: Color = Color::Rgb(165, 173, 203);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const MANTLE: Color = Color::Rgb(30, 32, 48);
const GREEN: Color = Color::Rgb(166, 218, 149);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const RED: Color = Color::Rgb(237, 135, 150);
const BLUE: Color = Color::Rgb(138, 173, 244);
const TEAL: Color = Color::Rgb(139, 213, 202);

// ── Helpers ──

fn percentile(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (p * sorted.len()) / 100;
    sorted[idx.min(sorted.len() - 1)]
}

fn format_duration(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        format!("{}ms", ms)
    }
}

fn duration_color(ms: u64) -> Color {
    if ms > 1000 {
        RED
    } else if ms > 500 {
        YELLOW
    } else {
        GREEN
    }
}

fn truncate_url(url: &str, max_w: usize) -> String {
    if url.len() <= max_w {
        return url.to_string();
    }
    if max_w <= 3 {
        return "...".to_string();
    }
    let mut result = String::new();
    let target = max_w - 3;
    for (width, ch) in url.chars().enumerate() {
        if width >= target {
            break;
        }
        result.push(ch);
    }
    result.push_str("...");
    result
}

fn extract_domain(url: &str) -> String {
    // Strip scheme
    let after_scheme = if let Some(pos) = url.find("://") {
        &url[pos + 3..]
    } else {
        url
    };
    // Take up to the first '/' or ':'
    after_scheme
        .split('/')
        .next()
        .unwrap_or(after_scheme)
        .split(':')
        .next()
        .unwrap_or(after_scheme)
        .to_string()
}

fn status_row(range_label: &str, count: usize, total: usize, color: Color) -> Row<'static> {
    let pct = if total > 0 {
        format!("{:.1}%", count as f64 / total as f64 * 100.0)
    } else {
        "0.0%".to_string()
    };
    Row::new(vec![
        Cell::from(range_label.to_string()).style(Style::default().fg(color)),
        Cell::from(count.to_string()).style(Style::default().fg(TEXT)),
        Cell::from(pct).style(Style::default().fg(SUBTEXT0)),
    ])
}

// ── Main draw ──

pub fn draw_network_stats(f: &mut Frame, app: &mut App) {
    app.layout.stats_slowest_regions.clear();

    // Collect data from network_store
    let mut total: usize = 0;
    let mut success: usize = 0;
    let mut failed: usize = 0;
    let mut in_progress: usize = 0;
    let mut durations: Vec<u64> = Vec::new();
    let mut url_durations: Vec<(usize, String, String, Option<u16>, u64)> = Vec::new(); // (store_idx, url, method, http_status, duration)
    let mut status_2xx: usize = 0;
    let mut status_3xx: usize = 0;
    let mut status_4xx: usize = 0;
    let mut status_5xx: usize = 0;
    let mut domain_map: HashMap<String, (usize, Vec<u64>, usize)> = HashMap::new(); // domain -> (count, durations, errors)

    for (store_idx, entry) in app.network_store.iter().enumerate() {
        total += 1;
        let domain = extract_domain(&entry.url);
        let dm = domain_map.entry(domain).or_insert((0, Vec::new(), 0));
        dm.0 += 1;

        match entry.status {
            NetworkStatus::Completed => {
                if let Some(code) = entry.http_status {
                    if code >= 500 {
                        status_5xx += 1;
                        dm.2 += 1;
                    } else if code >= 400 {
                        status_4xx += 1;
                        dm.2 += 1;
                        failed += 1;
                    } else if code >= 300 {
                        status_3xx += 1;
                        success += 1;
                    } else {
                        status_2xx += 1;
                        success += 1;
                    }
                } else {
                    success += 1;
                }
            }
            NetworkStatus::Failed => {
                failed += 1;
                dm.2 += 1;
            }
            NetworkStatus::Active => {
                in_progress += 1;
            }
            NetworkStatus::Pending => {
                in_progress += 1;
            }
        }

        if let Some(dur) = entry.duration {
            durations.push(dur);
            dm.1.push(dur);
            url_durations.push((
                store_idx,
                entry.url.clone(),
                entry.method.clone(),
                entry.http_status,
                dur,
            ));
        }
    }

    durations.sort();

    // Slowest top 5
    let mut slowest = url_durations.clone();
    slowest.sort_by(|a, b| b.4.cmp(&a.4));
    slowest.truncate(5);

    // Per-domain stats sorted by count desc
    let mut domain_stats: Vec<(String, usize, u64, f64)> = domain_map
        .into_iter()
        .map(|(domain, (count, durs, errors))| {
            let avg = if durs.is_empty() {
                0
            } else {
                durs.iter().sum::<u64>() / durs.len() as u64
            };
            let err_rate = if count > 0 {
                errors as f64 / count as f64 * 100.0
            } else {
                0.0
            };
            (domain, count, avg, err_rate)
        })
        .collect();
    domain_stats.sort_by(|a, b| b.1.cmp(&a.1));

    // Layout: title | summary + latency | slowest | status dist + domain
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Length(7), // summary + latency side-by-side
            Constraint::Length(9), // slowest top 5
            Constraint::Min(5),    // status dist + per-domain side-by-side
        ])
        .split(f.area());

    // ── Title bar ──
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " < Back ",
            Style::default()
                .fg(Color::Black)
                .bg(BLUE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  Network Statistics  ",
            Style::default().fg(Color::White).bg(MANTLE),
        ),
        Span::styled(
            format!("  Total: {}  ", total),
            Style::default().fg(TEXT).bg(MANTLE),
        ),
    ]))
    .style(Style::default().bg(MANTLE));
    f.render_widget(title, chunks[0]);

    // ── Summary + Latency (side-by-side) ──
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Summary table
    let summary_rows =
        vec![
            Row::new(vec![
                Cell::from("Total Requests").style(Style::default().fg(SUBTEXT0)),
                Cell::from(total.to_string())
                    .style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
            ]),
            Row::new(vec![
                Cell::from("Success").style(Style::default().fg(SUBTEXT0)),
                Cell::from(success.to_string()).style(Style::default().fg(GREEN)),
            ]),
            Row::new(vec![
                Cell::from("Failed").style(Style::default().fg(SUBTEXT0)),
                Cell::from(failed.to_string()).style(Style::default().fg(if failed > 0 {
                    RED
                } else {
                    TEXT
                })),
            ]),
            Row::new(vec![
                Cell::from("In-Progress").style(Style::default().fg(SUBTEXT0)),
                Cell::from(in_progress.to_string())
                    .style(Style::default().fg(if in_progress > 0 { YELLOW } else { TEXT })),
            ]),
        ];
    let summary_table = Table::new(
        summary_rows,
        [Constraint::Percentage(60), Constraint::Percentage(40)],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Summary ")
            .border_style(Style::default().fg(SURFACE0)),
    );
    f.render_widget(summary_table, top_cols[0]);

    // Latency table
    let avg_ms = if durations.is_empty() {
        0
    } else {
        durations.iter().sum::<u64>() / durations.len() as u64
    };
    let p50 = percentile(&durations, 50);
    let p95 = percentile(&durations, 95);
    let p99 = percentile(&durations, 99);

    let latency_rows = vec![
        Row::new(vec![
            Cell::from("Average").style(Style::default().fg(SUBTEXT0)),
            Cell::from(format_duration(avg_ms)).style(Style::default().fg(duration_color(avg_ms))),
        ]),
        Row::new(vec![
            Cell::from("P50").style(Style::default().fg(SUBTEXT0)),
            Cell::from(format_duration(p50)).style(Style::default().fg(duration_color(p50))),
        ]),
        Row::new(vec![
            Cell::from("P95").style(Style::default().fg(SUBTEXT0)),
            Cell::from(format_duration(p95)).style(Style::default().fg(duration_color(p95))),
        ]),
        Row::new(vec![
            Cell::from("P99").style(Style::default().fg(SUBTEXT0)),
            Cell::from(format_duration(p99)).style(Style::default().fg(duration_color(p99))),
        ]),
    ];
    let latency_table = Table::new(
        latency_rows,
        [Constraint::Percentage(50), Constraint::Percentage(50)],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Latency ")
            .border_style(Style::default().fg(SURFACE0)),
    );
    f.render_widget(latency_table, top_cols[1]);

    // ── Slowest Top 5 ──
    let slowest_header = Row::new(vec![
        Cell::from("#").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("URL").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Method").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Duration").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
    ]);

    let area_width = chunks[2].width.saturating_sub(2) as usize; // minus borders
    let url_max = area_width.saturating_sub(3 + 1 + 8 + 1 + 8 + 1 + 10); // #(3) + gaps + method(8) + status(8) + duration(10)

    // Collect store indices for click regions
    let slowest_store_indices: Vec<usize> = slowest.iter().map(|(idx, _, _, _, _)| *idx).collect();

    let slowest_rows: Vec<Row> = slowest
        .iter()
        .enumerate()
        .map(|(i, (_store_idx, url, method, http_status, dur))| {
            let status_text = http_status
                .map(|c| c.to_string())
                .unwrap_or_else(|| "-".to_string());
            let dur_c = duration_color(*dur);
            Row::new(vec![
                Cell::from(format!("{}", i + 1)).style(Style::default().fg(SURFACE0)),
                Cell::from(truncate_url(url, url_max)).style(Style::default().fg(TEXT)),
                Cell::from(method.clone()).style(Style::default().fg(TEAL)),
                Cell::from(status_text).style(Style::default().fg(SUBTEXT0)),
                Cell::from(format_duration(*dur)).style(Style::default().fg(dur_c)),
            ])
        })
        .collect();

    let slowest_table = Table::new(
        slowest_rows,
        [
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(slowest_header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Slowest Top 5 ")
            .border_style(Style::default().fg(SURFACE0)),
    );
    f.render_widget(slowest_table, chunks[2]);

    // Register click regions for slowest rows (header=1 row + border=1 row offset)
    let slowest_area = chunks[2];
    for (i, store_idx) in slowest_store_indices.iter().enumerate() {
        let row_y = slowest_area.y + 2 + i as u16; // +1 border +1 header
        if row_y < slowest_area.y + slowest_area.height.saturating_sub(1) {
            app.layout.stats_slowest_regions.push((
                *store_idx,
                row_y,
                slowest_area.x,
                slowest_area.x + slowest_area.width,
            ));
        }
    }

    // ── Status Code Distribution + Per-Domain (side-by-side) ──
    let bottom_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[3]);

    // Status code distribution
    let completed = status_2xx + status_3xx + status_4xx + status_5xx;
    let status_rows = vec![
        status_row("2xx", status_2xx, completed, GREEN),
        status_row("3xx", status_3xx, completed, BLUE),
        status_row("4xx", status_4xx, completed, YELLOW),
        status_row("5xx", status_5xx, completed, RED),
    ];

    let status_header = Row::new(vec![
        Cell::from("Range").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Count").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Percentage").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
    ]);

    let status_table = Table::new(
        status_rows,
        [
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Min(10),
        ],
    )
    .header(status_header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Status Codes ")
            .border_style(Style::default().fg(SURFACE0)),
    );
    f.render_widget(status_table, bottom_cols[0]);

    // Per-domain stats
    let domain_header = Row::new(vec![
        Cell::from("Domain").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Requests").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Avg Time").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
        Cell::from("Error Rate").style(Style::default().fg(SUBTEXT0).add_modifier(Modifier::BOLD)),
    ]);

    let max_domain_rows = bottom_cols[1].height.saturating_sub(3) as usize; // borders + header
    let domain_rows: Vec<Row> = domain_stats
        .iter()
        .take(max_domain_rows)
        .map(|(domain, count, avg, err_rate)| {
            let err_color = if *err_rate > 10.0 {
                RED
            } else if *err_rate > 0.0 {
                YELLOW
            } else {
                GREEN
            };
            Row::new(vec![
                Cell::from(truncate_url(domain, 30)).style(Style::default().fg(TEAL)),
                Cell::from(count.to_string()).style(Style::default().fg(TEXT)),
                Cell::from(format_duration(*avg)).style(Style::default().fg(duration_color(*avg))),
                Cell::from(format!("{:.1}%", err_rate)).style(Style::default().fg(err_color)),
            ])
        })
        .collect();

    let domain_table = Table::new(
        domain_rows,
        [
            Constraint::Min(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(12),
        ],
    )
    .header(domain_header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Per-Domain Stats ")
            .border_style(Style::default().fg(SURFACE0)),
    );
    f.render_widget(domain_table, bottom_cols[1]);
}
