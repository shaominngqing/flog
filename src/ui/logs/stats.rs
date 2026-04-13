use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::domain::LogLevel;

pub fn draw_stats(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // nav bar
            Constraint::Length(10), // level stats
            Constraint::Min(5),     // tag ranking
        ])
        .split(f.area());

    let accent = Color::Rgb(138, 173, 244);
    let bar_bg = Color::Rgb(30, 32, 48);

    let (total, filtered) = app
        .stats_snapshot
        .as_ref()
        .map(|s| (s.total, s.filtered))
        .unwrap_or((app.store.len(), 0));

    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " < Back ",
            Style::default()
                .fg(Color::Black)
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  Statistics  ",
            Style::default().fg(Color::White).bg(bar_bg),
        ),
        Span::styled(
            format!("  Total: {}  Filtered: {}  ", total, filtered),
            Style::default().fg(Color::Rgb(202, 211, 245)).bg(bar_bg),
        ),
    ]))
    .style(Style::default().bg(bar_bg));
    f.render_widget(title, chunks[0]);

    draw_level_stats(f, app, chunks[1]);
    draw_tag_ranking(f, app, chunks[2]);
}

fn draw_level_stats(f: &mut Frame, app: &App, area: Rect) {
    let levels = [
        (LogLevel::System, "SYSTEM", Color::Rgb(54, 58, 79)),
        (LogLevel::Debug, "DEBUG", Color::Gray),
        (LogLevel::Info, "INFO", Color::Blue),
        (LogLevel::Warning, "WARN", Color::Yellow),
        (LogLevel::Error, "ERROR", Color::Red),
    ];

    let bars: Vec<Bar> = if let Some(snap) = &app.stats_snapshot {
        levels
            .iter()
            .map(|(level, label, color)| {
                let count = snap
                    .level_counts
                    .iter()
                    .find(|(l, _)| l == level)
                    .map(|(_, c)| *c)
                    .unwrap_or(0);
                Bar::default()
                    .value(count)
                    .label(Line::from(*label))
                    .style(Style::default().fg(*color))
                    .value_style(Style::default().fg(Color::White).bg(*color))
            })
            .collect()
    } else {
        Vec::new()
    };

    let chart = BarChart::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Log Levels ")
                .border_style(Style::default().fg(Color::Rgb(54, 58, 79))),
        )
        .data(BarGroup::default().bars(&bars))
        .bar_width(10)
        .bar_gap(2);

    f.render_widget(chart, area);
}

fn draw_tag_ranking(f: &mut Frame, app: &App, area: Rect) {
    let max_rows = area.height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    if let Some(snap) = &app.stats_snapshot {
        for (i, (tag, count)) in snap.tag_ranking.iter().take(max_rows).enumerate() {
            let bar_max = 30;
            let max_count = snap.tag_ranking.first().map(|(_, c)| *c).unwrap_or(1);
            let bar_len = (count * bar_max) / max_count.max(1);
            let bar: String = "\u{2588}".repeat(bar_len);

            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {:>2}. ", i + 1),
                    Style::default().fg(Color::Rgb(54, 58, 79)),
                ),
                Span::styled(
                    format!("{:<14}", tag),
                    Style::default().fg(Color::Rgb(139, 213, 202)),
                ),
                Span::styled(format!("{:>6} ", count), Style::default().fg(Color::White)),
                Span::styled(bar, Style::default().fg(Color::Blue)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Tag Ranking ")
            .border_style(Style::default().fg(Color::Rgb(54, 58, 79))),
    );

    f.render_widget(paragraph, area);
}
