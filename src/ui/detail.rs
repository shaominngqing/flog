//! Detail side panel — shows selected log entry with JSON formatting and fold/unfold.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use regex::Regex;
use std::sync::LazyLock;

use crate::app::App;
use crate::domain::LogLevel;

const MANTLE: Color   = Color::Rgb(30, 32, 48);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const BLUE: Color     = Color::Rgb(138, 173, 244);
const TEAL: Color     = Color::Rgb(139, 213, 202);
const GREEN: Color    = Color::Rgb(166, 218, 149);
const YELLOW: Color   = Color::Rgb(238, 212, 159);
const PEACH: Color    = Color::Rgb(245, 169, 127);
const RED: Color      = Color::Rgb(237, 135, 150);
const MAUVE: Color    = Color::Rgb(198, 160, 246);
const PINK: Color     = Color::Rgb(245, 189, 230);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
const LAVENDER: Color = Color::Rgb(183, 189, 248);

const STR_COLOR: Color = GREEN;
const NUM_COLOR: Color = PEACH;
const BOOL_COLOR: Color = PINK;
const NULL_COLOR: Color = OVERLAY0;
const COMMA_COLOR: Color = SURFACE0;
const FOLD_COLOR: Color = OVERLAY0;

const DEPTH_COLORS: [Color; 6] = [MAUVE, BLUE, TEAL, YELLOW, SAPPHIRE, LAVENDER];
const DEPTH_BRACE: [Color; 6] = [
    Color::Rgb(110, 115, 141), Color::Rgb(100, 105, 131),
    Color::Rgb(90, 95, 121), Color::Rgb(80, 85, 111),
    Color::Rgb(73, 77, 100), Color::Rgb(54, 58, 79),
];

fn key_color(depth: usize) -> Color { DEPTH_COLORS[depth % DEPTH_COLORS.len()] }
fn brace_color(depth: usize) -> Color { DEPTH_BRACE[depth % DEPTH_BRACE.len()] }

static JSON_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^"([^"]+)"\s*:\s*"#).unwrap()
});

// ══════════════════════════════════════
//  Side Panel Renderer
// ══════════════════════════════════════

pub fn draw_side_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let store_idx = match app.selected_store_index() {
        Some(idx) => idx,
        None => {
            let block = Block::default()
                .title(" Details ")
                .borders(Borders::LEFT)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(SURFACE0))
                .style(Style::default().bg(MANTLE));
            f.render_widget(
                Paragraph::new(Line::from(Span::styled("  Select a log entry", Style::default().fg(OVERLAY0)))).block(block),
                area,
            );
            return;
        }
    };

    let entry = match app.store.get(store_idx) { Some(e) => e.clone(), None => return };

    let inner_h = area.height.saturating_sub(2) as usize;
    let inner_w = area.width.saturating_sub(2) as usize;

    // ── Header ──
    let _lc = match entry.level {
        LogLevel::Error => RED, LogLevel::Warning => YELLOW, LogLevel::Info => BLUE, _ => OVERLAY0,
    };
    let (lfg, lbg) = match entry.level {
        LogLevel::Info => (MANTLE, BLUE), LogLevel::Warning => (MANTLE, YELLOW),
        LogLevel::Error => (MANTLE, RED), _ => (OVERLAY0, Color::Reset),
    };
    let ls = if lbg == Color::Reset { Style::default().fg(lfg) } else { Style::default().fg(lfg).bg(lbg).add_modifier(Modifier::BOLD) };

    let mut all_lines: Vec<Line> = Vec::new();
    all_lines.push(Line::from(vec![
        Span::styled(format!(" {} ", entry.level.as_str()), ls),
        Span::styled(format!("  {}", entry.tag), Style::default().fg(TEAL)),
    ]));
    if !entry.timestamp.is_empty() {
        all_lines.push(Line::from(Span::styled(format!("  {}", entry.timestamp), Style::default().fg(OVERLAY0))));
    }
    all_lines.push(Line::from(Span::styled("─".repeat(inner_w), Style::default().fg(SURFACE0))));

    // Store header line count for click handling (+ 1 for block border top)
    app.detail.header_lines = all_lines.len() + 1;

    // ── Body with fold/unfold ──
    let full_msg = entry.full_message();
    let fmt_lines = bracket_format(&full_msg);

    app.detail.foldable.clear();
    for (i, fl) in fmt_lines.iter().enumerate() {
        if fl.close_line.is_some() { app.detail.foldable.insert(i); }
    }

    let collapsed = &app.detail.collapsed;
    let body_height = inner_h.saturating_sub(all_lines.len());
    let mut row_to_source: Vec<usize> = Vec::new();
    let mut skip_until: Option<usize> = None;
    let mut di: usize = 0;

    for (si, fl) in fmt_lines.iter().enumerate() {
        if let Some(u) = skip_until { if si <= u { continue; } skip_until = None; }

        if collapsed.contains(&si) {
            if let Some(close) = fl.close_line {
                if di >= app.detail.scroll && all_lines.len().saturating_sub(3) < body_height {
                    let ind = "  ".repeat(fl.depth);
                    let br = if fl.text.trim_start().starts_with('[') { "[" } else { "{" };
                    let cb = if br == "[" { "]" } else { "}" };
                    let n = close.saturating_sub(si).saturating_sub(1);
                    all_lines.push(Line::from(vec![
                        Span::styled(format!("{} ", ind), Style::default()),
                        Span::styled("▶ ", Style::default().fg(BLUE)),
                        Span::styled(format!("{}...{}", br, cb), Style::default().fg(FOLD_COLOR)),
                        Span::styled(format!(" ({})", n), Style::default().fg(OVERLAY0)),
                    ]));
                    row_to_source.push(si);
                }
                di += 1; skip_until = Some(close); continue;
            }
        }

        if di >= app.detail.scroll && all_lines.len().saturating_sub(3) < body_height {
            if fl.close_line.is_some() {
                let mut spans = vec![
                    Span::styled("  ".repeat(fl.depth), Style::default()),
                    Span::styled("▼ ", Style::default().fg(BLUE)),
                ];
                spans.extend(colorize_content_depth(fl.text.trim_start(), fl.depth));
                all_lines.push(Line::from(spans));
            } else {
                all_lines.push(colorize_line_depth(&fl.text, fl.depth));
            }
            row_to_source.push(si);
        }
        di += 1;
    }

    app.detail.row_to_source = row_to_source;
    app.detail.total_lines = fmt_lines.len();

    let block = Block::default()
        .title(" Details ")
        .title_style(Style::default().fg(BLUE).add_modifier(Modifier::BOLD))
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE0))
        .style(Style::default().bg(MANTLE));

    f.render_widget(
        Paragraph::new(all_lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area,
    );
}

// ══════════════════════════════════════
//  Bracket formatter
// ══════════════════════════════════════

struct FmtLine { text: String, depth: usize, close_line: Option<usize> }

fn bracket_format(text: &str) -> Vec<FmtLine> {
    let ss = text.find(|c: char| c == '{' || c == '[');
    let mut lines: Vec<FmtLine> = Vec::new();
    if let Some(sp) = ss {
        let prefix = text[..sp].trim();
        if !prefix.is_empty() { lines.push(FmtLine { text: prefix.to_string(), depth: 0, close_line: None }); }
        let raw = indent_brackets(&text[sp..]);
        let base = lines.len();
        for r in &raw { lines.push(FmtLine { text: r.clone(), depth: 0, close_line: None }); }
        let mut stack: Vec<usize> = Vec::new();
        for i in base..lines.len() {
            let tl = lines[i].text.len(); let sl = lines[i].text.trim_start().len();
            lines[i].depth = (tl - sl) / 2;
            let sc = lines[i].text.trim_start().starts_with('}') || lines[i].text.trim_start().starts_with(']');
            let eo = lines[i].text.trim_end().ends_with('{') || lines[i].text.trim_end().ends_with('[')
                || lines[i].text.trim() == "{" || lines[i].text.trim() == "[";
            if eo { stack.push(i); }
            if sc { if let Some(oi) = stack.pop() { lines[oi].close_line = Some(i); } }
        }
    } else {
        for l in text.lines() { lines.push(FmtLine { text: l.to_string(), depth: 0, close_line: None }); }
    }
    lines
}

fn indent_brackets(text: &str) -> Vec<String> {
    let mut r: Vec<String> = Vec::new();
    let (mut d, mut cur, mut ins, mut esc) = (0usize, String::new(), false, false);
    let ch: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if esc { cur.push(c); esc = false; i += 1; continue; }
        if c == '\\' && ins { cur.push(c); esc = true; i += 1; continue; }
        if c == '"' { ins = !ins; cur.push(c); i += 1; continue; }
        if ins { cur.push(c); i += 1; continue; }
        match c {
            '{' | '[' => {
                cur.push(c);
                let nx = ch[i+1..].iter().position(|&x| !x.is_whitespace());
                let em = nx.map(|p| ch[i+1+p] == '}' || ch[i+1+p] == ']').unwrap_or(false);
                if !em { flush(&mut r, &cur, d); cur.clear(); d += 1; }
                i += 1;
            }
            '}' | ']' => {
                if !cur.trim().is_empty() { flush(&mut r, &cur, d); cur.clear(); }
                if d > 0 { d -= 1; }
                cur.push(c);
                let mut j = i + 1;
                while j < ch.len() && (ch[j] == ',' || ch[j].is_whitespace()) {
                    if ch[j] == ',' { cur.push(','); j += 1; break; } j += 1;
                }
                flush(&mut r, &cur, d); cur.clear(); i = j;
            }
            ',' => { cur.push(','); flush(&mut r, &cur, d); cur.clear(); i += 1; }
            ' ' | '\n' | '\r' | '\t' => { if !cur.is_empty() && !cur.ends_with(' ') { cur.push(' '); } i += 1; }
            _ => { cur.push(c); i += 1; }
        }
    }
    if !cur.trim().is_empty() { flush(&mut r, &cur, d); }
    r
}

fn flush(r: &mut Vec<String>, c: &str, d: usize) {
    let t = c.trim(); if t.is_empty() { return; }
    r.push(format!("{}{}", "  ".repeat(d), t));
}

// ══════════════════════════════════════
//  Syntax coloring
// ══════════════════════════════════════

fn colorize_line_depth(line: &str, depth: usize) -> Line<'static> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let mut spans: Vec<Span<'static>> = Vec::new();
    if indent > 0 { spans.push(Span::styled(" ".repeat(indent), Style::default())); }
    spans.extend(colorize_content_depth(trimmed, depth));
    Line::from(spans)
}

fn colorize_content_depth(trimmed: &str, depth: usize) -> Vec<Span<'static>> {
    let kc = key_color(depth); let bc = brace_color(depth);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if trimmed == "{" || trimmed == "}" || trimmed == "}," || trimmed == "[" || trimmed == "]" || trimmed == "]," {
        spans.push(Span::styled(trimmed.to_string(), Style::default().fg(bc))); return spans;
    }
    if let Some(caps) = JSON_KEY_RE.captures(trimmed) {
        let key = caps.get(1).unwrap().as_str();
        let after = &trimmed[caps.get(0).unwrap().end()..];
        spans.push(Span::styled(format!("\"{}\"", key), Style::default().fg(kc)));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
        spans.extend(colorize_value(after)); return spans;
    }
    if let Some(cp) = trimmed.find(": ") {
        let key = &trimmed[..cp]; let val = &trimmed[cp + 2..];
        if key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '"') {
            spans.push(Span::styled(key.to_string(), Style::default().fg(kc)));
            spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
            spans.extend(colorize_value(val)); return spans;
        }
    }
    spans.extend(colorize_value(trimmed)); spans
}

fn colorize_value(text: &str) -> Vec<Span<'static>> {
    let bare = text.trim_end_matches(','); let comma = text.len() > bare.len(); let v = bare.trim();
    let mut spans = Vec::new();
    if v.starts_with('"') && v.ends_with('"') { spans.push(Span::styled(v.to_string(), Style::default().fg(STR_COLOR))); }
    else if v == "null" { spans.push(Span::styled(v.to_string(), Style::default().fg(NULL_COLOR).add_modifier(Modifier::ITALIC))); }
    else if v == "true" || v == "false" { spans.push(Span::styled(v.to_string(), Style::default().fg(BOOL_COLOR))); }
    else if v.parse::<f64>().is_ok() { spans.push(Span::styled(v.to_string(), Style::default().fg(NUM_COLOR))); }
    else if v.starts_with('{') || v.starts_with('[') { spans.push(Span::styled(v.to_string(), Style::default().fg(brace_color(0)))); }
    else { spans.push(Span::styled(v.to_string(), Style::default().fg(STR_COLOR))); }
    if comma { spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR))); }
    spans
}
