//! Timeline density heatmap.

use crate::app::App;
use crate::domain::LogLevel;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

const BASE: Color = Color::Rgb(36, 39, 58);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const BLUE: Color = Color::Rgb(138, 173, 244);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const RED: Color = Color::Rgb(237, 135, 150);

const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn draw_timeline(f: &mut Frame, app: &mut App, area: Rect) {
    let width = area.width.saturating_sub(2) as usize;
    if width == 0 || app.store.is_empty() {
        f.render_widget(
            Paragraph::new("")
                .block(
                    Block::default()
                        .borders(Borders::TOP)
                        .border_style(Style::default().fg(SURFACE0)),
                )
                .style(Style::default().bg(BASE)),
            area,
        );
        return;
    }

    let total = app.store.len();
    let bs = (total as f64 / width as f64).ceil().max(1.0) as usize;
    let mut buckets: Vec<(u32, bool, bool)> = Vec::with_capacity(width);
    let mut idx = 0;

    for _ in 0..width {
        let (mut c, mut e, mut w) = (0u32, false, false);
        for _ in 0..bs {
            if idx >= total {
                break;
            }
            if let Some(entry) = app.store.get(idx) {
                c += 1;
                match entry.level {
                    LogLevel::Error => e = true,
                    LogLevel::Warning => w = true,
                    _ => {}
                }
            }
            idx += 1;
        }
        buckets.push((c, e, w));
    }

    let mx = buckets.iter().map(|b| b.0).max().unwrap_or(1).max(1);
    let mut bar_spans: Vec<Span> = Vec::with_capacity(width);

    for &(count, has_err, has_warn) in &buckets {
        let bi = if count == 0 {
            0
        } else {
            ((count as f64 / mx as f64) * 7.0) as usize
        };
        let color = if has_err {
            RED
        } else if has_warn {
            YELLOW
        } else {
            BLUE
        };
        bar_spans.push(Span::styled(
            BARS[bi.min(7)].to_string(),
            Style::default().fg(color).bg(BASE),
        ));
    }

    let ft = app
        .store
        .get(0)
        .map(|e| e.timestamp.clone())
        .unwrap_or_default();
    let lt = app
        .store
        .get(total.saturating_sub(1))
        .map(|e| e.timestamp.clone())
        .unwrap_or_default();

    let bar_line = Line::from(bar_spans);
    let time_line = Line::from(vec![
        Span::styled(format!(" {}", ft), Style::default().fg(OVERLAY0)),
        Span::styled(
            format!(
                "{:>w$}",
                format!("{} ", lt),
                w = width.saturating_sub(ft.len() + 1)
            ),
            Style::default().fg(OVERLAY0),
        ),
    ]);

    f.render_widget(
        Paragraph::new(vec![bar_line, time_line])
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(SURFACE0)),
            )
            .style(Style::default().bg(BASE)),
        area,
    );
}

pub fn click_to_offset(x: u16, total_width: u16, filtered_count: usize) -> usize {
    let w = total_width.saturating_sub(2) as f64;
    if w <= 0.0 || filtered_count == 0 {
        return 0;
    }
    ((x as f64).min(w) / w * filtered_count as f64) as usize
}
