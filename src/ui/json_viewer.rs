//! Shared JSON viewer component with collapsible tree display.
//!
//! Extracted from logs/detail.rs for reuse in network detail panels.

use std::collections::HashSet;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use regex::Regex;
use std::sync::LazyLock;

use super::{
    BLUE, GREEN, LAVENDER, MAUVE, OVERLAY0, PEACH, PINK, SAPPHIRE, SURFACE0, TEAL, TEXT, YELLOW,
};

// ══════════════════════════════════════
//  Color Constants
// ══════════════════════════════════════

const STR_COLOR: Color = GREEN;
const NUM_COLOR: Color = PEACH;
const BOOL_COLOR: Color = PINK;
const NULL_COLOR: Color = OVERLAY0;
const COMMA_COLOR: Color = SURFACE0;
const FOLD_COLOR: Color = OVERLAY0;

const DEPTH_COLORS: [Color; 6] = [MAUVE, BLUE, TEAL, YELLOW, SAPPHIRE, LAVENDER];
const DEPTH_BRACE: [Color; 6] = [
    Color::Rgb(110, 115, 141),
    Color::Rgb(100, 105, 131),
    Color::Rgb(90, 95, 121),
    Color::Rgb(80, 85, 111),
    Color::Rgb(73, 77, 100),
    Color::Rgb(54, 58, 79),
];

fn key_color(depth: usize) -> Color {
    DEPTH_COLORS[depth % DEPTH_COLORS.len()]
}
fn brace_color(depth: usize) -> Color {
    DEPTH_BRACE[depth % DEPTH_BRACE.len()]
}

static JSON_KEY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^"([^"]+)"\s*:\s*"#).unwrap());

// ══════════════════════════════════════
//  Data Structures
// ══════════════════════════════════════

/// A formatted line from bracket formatting.
pub struct FmtLine {
    pub text: String,
    pub depth: usize,
    pub close_line: Option<usize>,
}

/// State for the JSON viewer's fold/unfold behavior.
#[derive(Default, Clone)]
pub struct JsonViewerState {
    /// Set of source line indices that are collapsed.
    pub collapsed: HashSet<usize>,
    /// Set of source line indices that ARE foldable (have a matching close bracket).
    pub foldable: HashSet<usize>,
    /// Maps display row -> source line index.
    pub row_to_source: Vec<usize>,
    /// Total number of source lines.
    pub total_lines: usize,
}

// ══════════════════════════════════════
//  Public Functions
// ══════════════════════════════════════

/// Format text with bracket indentation for JSON-like content.
pub fn bracket_format(text: &str) -> Vec<FmtLine> {
    let ss = text.find(['{', '[']);
    let mut lines: Vec<FmtLine> = Vec::new();
    if let Some(sp) = ss {
        let prefix = text[..sp].trim();
        if !prefix.is_empty() {
            lines.push(FmtLine {
                text: prefix.to_string(),
                depth: 0,
                close_line: None,
            });
        }
        let raw = indent_brackets(&text[sp..]);
        let base = lines.len();
        for r in &raw {
            lines.push(FmtLine {
                text: r.clone(),
                depth: 0,
                close_line: None,
            });
        }
        let mut stack: Vec<usize> = Vec::new();
        for i in base..lines.len() {
            let tl = lines[i].text.len();
            let sl = lines[i].text.trim_start().len();
            lines[i].depth = (tl - sl) / 2;
            let sc = lines[i].text.trim_start().starts_with('}')
                || lines[i].text.trim_start().starts_with(']');
            let eo = lines[i].text.trim_end().ends_with('{')
                || lines[i].text.trim_end().ends_with('[')
                || lines[i].text.trim() == "{"
                || lines[i].text.trim() == "[";
            if eo {
                stack.push(i);
            }
            if sc {
                if let Some(oi) = stack.pop() {
                    lines[oi].close_line = Some(i);
                }
            }
        }
    } else {
        for l in text.lines() {
            lines.push(FmtLine {
                text: l.to_string(),
                depth: 0,
                close_line: None,
            });
        }
    }
    lines
}

/// Initialize fold state, collapsing everything at depth >= auto_expand_depth.
pub fn init_state(fmt_lines: &[FmtLine], auto_expand_depth: usize) -> JsonViewerState {
    let mut state = JsonViewerState {
        total_lines: fmt_lines.len(),
        ..Default::default()
    };
    for (i, fl) in fmt_lines.iter().enumerate() {
        if fl.close_line.is_some() {
            state.foldable.insert(i);
            if fl.depth >= auto_expand_depth {
                state.collapsed.insert(i);
            }
        }
    }
    state
}

/// Toggle a line's collapsed state. Returns true if the line was foldable.
pub fn toggle_fold(state: &mut JsonViewerState, source_line: usize) -> bool {
    if !state.foldable.contains(&source_line) {
        return false;
    }
    if !state.collapsed.remove(&source_line) {
        state.collapsed.insert(source_line);
    }
    true
}

/// Render formatted JSON with fold/unfold, depth-aware colors.
/// Returns the rendered lines and updates state.row_to_source.
/// `max_width` controls line wrapping (0 = no limit).
pub fn render_json(
    fmt_lines: &[FmtLine],
    state: &mut JsonViewerState,
    scroll: usize,
    max_lines: usize,
    max_width: usize,
) -> Vec<Line<'static>> {
    let mut all_lines: Vec<Line<'static>> = Vec::new();
    let mut row_to_source: Vec<usize> = Vec::new();
    let mut skip_until: Option<usize> = None;
    let mut di: usize = 0; // display index (before scroll/take)

    for (si, fl) in fmt_lines.iter().enumerate() {
        if let Some(u) = skip_until {
            if si <= u {
                continue;
            }
            skip_until = None;
        }

        if state.collapsed.contains(&si) {
            if let Some(close) = fl.close_line {
                // Render collapsed marker
                if di >= scroll && all_lines.len() < max_lines {
                    let ind = "  ".repeat(fl.depth);
                    let br = if fl.text.trim_start().starts_with('[') {
                        "["
                    } else {
                        "{"
                    };
                    let cb = if br == "[" { "]" } else { "}" };
                    // Count direct children (entries at depth+1) instead of line count
                let child_depth = fl.depth + 1;
                let n = fmt_lines[si + 1..close]
                    .iter()
                    .filter(|f| f.depth == child_depth)
                    .count();
                    all_lines.push(Line::from(vec![
                        Span::styled(format!("{} ", ind), Style::default()),
                        Span::styled("\u{25b6} ", Style::default().fg(BLUE)), // ▶
                        Span::styled(format!("{}...{}", br, cb), Style::default().fg(FOLD_COLOR)),
                        Span::styled(format!(" ({})", n), Style::default().fg(OVERLAY0)),
                    ]));
                    row_to_source.push(si);
                }
                di += 1;
                skip_until = Some(close);
                continue;
            }
        }

        // Not collapsed or not foldable
        if di >= scroll && all_lines.len() < max_lines {
            let line = if fl.close_line.is_some() {
                // Foldable line with expansion arrow
                let mut spans = vec![
                    Span::styled("  ".repeat(fl.depth), Style::default()),
                    Span::styled("\u{25bc} ", Style::default().fg(BLUE)), // ▼
                ];
                spans.extend(colorize_content_depth(fl.text.trim_start(), fl.depth));
                Line::from(spans)
            } else {
                colorize_line_depth(&fl.text, fl.depth)
            };

            // Wrap long lines if max_width > 0
            if max_width > 0 {
                let raw_text = line_to_string(&line);
                let text_width = unicode_width::UnicodeWidthStr::width(raw_text.as_str());
                if text_width > max_width {
                    let indent = "  ".repeat(fl.depth + 1);
                    let wrapped = super::wrap_text(&raw_text, max_width, 30);
                    for (wi, wl) in wrapped.iter().enumerate() {
                        if all_lines.len() >= max_lines {
                            break;
                        }
                        if wi == 0 {
                            // First line: re-colorize the truncated portion
                            let first_line = colorize_line_depth(wl, fl.depth);
                            all_lines.push(first_line);
                        } else {
                            // Continuation lines with extra indent
                            all_lines.push(Line::from(Span::styled(
                                format!("{}{}", indent, wl),
                                Style::default().fg(STR_COLOR),
                            )));
                        }
                        row_to_source.push(si);
                    }
                } else {
                    all_lines.push(line);
                    row_to_source.push(si);
                }
            } else {
                all_lines.push(line);
                row_to_source.push(si);
            }
        }
        di += 1;
    }

    state.row_to_source = row_to_source;
    all_lines
}

/// Extract plain text from a Line for width measurement.
fn line_to_string(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

// ══════════════════════════════════════
//  Internal Functions
// ══════════════════════════════════════

fn indent_brackets(text: &str) -> Vec<String> {
    let mut r: Vec<String> = Vec::new();
    let (mut d, mut cur, mut ins, mut esc) = (0usize, String::new(), false, false);
    let ch: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if esc {
            cur.push(c);
            esc = false;
            i += 1;
            continue;
        }
        if c == '\\' && ins {
            cur.push(c);
            esc = true;
            i += 1;
            continue;
        }
        if c == '"' {
            ins = !ins;
            cur.push(c);
            i += 1;
            continue;
        }
        if ins {
            cur.push(c);
            i += 1;
            continue;
        }
        match c {
            '{' | '[' => {
                cur.push(c);
                let nx = ch[i + 1..].iter().position(|&x| !x.is_whitespace());
                let em = nx
                    .map(|p| ch[i + 1 + p] == '}' || ch[i + 1 + p] == ']')
                    .unwrap_or(false);
                if !em {
                    flush(&mut r, &cur, d);
                    cur.clear();
                    d += 1;
                }
                i += 1;
            }
            '}' | ']' => {
                if !cur.trim().is_empty() {
                    flush(&mut r, &cur, d);
                    cur.clear();
                }
                d = d.saturating_sub(1);
                cur.push(c);
                let mut j = i + 1;
                while j < ch.len() && (ch[j] == ',' || ch[j].is_whitespace()) {
                    if ch[j] == ',' {
                        cur.push(',');
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                flush(&mut r, &cur, d);
                cur.clear();
                i = j;
            }
            ',' => {
                cur.push(',');
                flush(&mut r, &cur, d);
                cur.clear();
                i += 1;
            }
            ' ' | '\n' | '\r' | '\t' => {
                if !cur.is_empty() && !cur.ends_with(' ') {
                    cur.push(' ');
                }
                i += 1;
            }
            _ => {
                cur.push(c);
                i += 1;
            }
        }
    }
    if !cur.trim().is_empty() {
        flush(&mut r, &cur, d);
    }
    r
}

fn flush(r: &mut Vec<String>, c: &str, d: usize) {
    let t = c.trim();
    if t.is_empty() {
        return;
    }
    r.push(format!("{}{}", "  ".repeat(d), t));
}

fn colorize_line_depth(line: &str, depth: usize) -> Line<'static> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let mut spans: Vec<Span<'static>> = Vec::new();
    if indent > 0 {
        spans.push(Span::styled(" ".repeat(indent), Style::default()));
    }
    spans.extend(colorize_content_depth(trimmed, depth));
    Line::from(spans)
}

fn colorize_content_depth(trimmed: &str, depth: usize) -> Vec<Span<'static>> {
    let kc = key_color(depth);
    let bc = brace_color(depth);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if trimmed == "{"
        || trimmed == "}"
        || trimmed == "},"
        || trimmed == "["
        || trimmed == "]"
        || trimmed == "],"
    {
        spans.push(Span::styled(trimmed.to_string(), Style::default().fg(bc)));
        return spans;
    }
    if let Some(caps) = JSON_KEY_RE.captures(trimmed) {
        let key = caps.get(1).unwrap().as_str();
        let after = &trimmed[caps.get(0).unwrap().end()..];
        spans.push(Span::styled(
            format!("\"{}\"", key),
            Style::default().fg(kc),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
        spans.extend(colorize_value(after));
        return spans;
    }
    if let Some(cp) = trimmed.find(": ") {
        let key = &trimmed[..cp];
        let val = &trimmed[cp + 2..];
        if key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '"')
        {
            spans.push(Span::styled(key.to_string(), Style::default().fg(kc)));
            spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
            spans.extend(colorize_value(val));
            return spans;
        }
    }
    spans.extend(colorize_value(trimmed));
    spans
}

fn colorize_value(text: &str) -> Vec<Span<'static>> {
    let bare = text.trim_end_matches(',');
    let comma = text.len() > bare.len();
    let v = bare.trim();
    let mut spans = Vec::new();
    if v.starts_with('"') && v.ends_with('"') {
        spans.push(Span::styled(v.to_string(), Style::default().fg(STR_COLOR)));
    } else if v == "null" {
        spans.push(Span::styled(
            v.to_string(),
            Style::default()
                .fg(NULL_COLOR)
                .add_modifier(Modifier::ITALIC),
        ));
    } else if v == "true" || v == "false" {
        spans.push(Span::styled(v.to_string(), Style::default().fg(BOOL_COLOR)));
    } else if v.parse::<f64>().is_ok() {
        spans.push(Span::styled(v.to_string(), Style::default().fg(NUM_COLOR)));
    } else if v.starts_with('{') || v.starts_with('[') {
        spans.push(Span::styled(
            v.to_string(),
            Style::default().fg(brace_color(0)),
        ));
    } else {
        spans.push(Span::styled(v.to_string(), Style::default().fg(STR_COLOR)));
    }
    if comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }
    spans
}

// ══════════════════════════════════════
//  Raw JSON Text Syntax Highlighting
// ══════════════════════════════════════

/// Colorize raw text (typically JSON) with syntax highlighting.
///
/// Performs character-by-character tokenization to produce colored `Line`s.
/// Tracks brace/bracket depth across lines for depth-aware key and brace colors.
/// Non-JSON text renders safely in default color.
pub fn colorize_json_text(text: &str) -> Vec<Line<'static>> {
    let default_style = Style::default().fg(TEXT);
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut result: Vec<Line<'static>> = Vec::new();

    for line in text.split('\n') {
        if line.is_empty() {
            result.push(Line::raw(""));
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut buf = String::new();
        let mut buf_style = default_style;
        let mut i = 0;

        // Flush accumulated buffer into spans
        macro_rules! flush {
            () => {
                if !buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut buf), buf_style));
                }
            };
        }

        // If we're continuing a string from a previous line, keep string coloring
        if in_string {
            buf_style = Style::default().fg(STR_COLOR);
            while i < len {
                let c = chars[i];
                if c == '\\' && i + 1 < len {
                    buf.push(c);
                    buf.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == '"' {
                    buf.push(c);
                    i += 1;
                    in_string = false;
                    flush!();
                    buf_style = default_style;
                    break;
                }
                buf.push(c);
                i += 1;
            }
            // If still in_string at end of line, flush what we have
            if in_string {
                flush!();
                result.push(Line::from(spans));
                continue;
            }
        }

        while i < len {
            let c = chars[i];

            match c {
                '"' => {
                    flush!();
                    // Collect the entire string including quotes
                    let mut s = String::new();
                    s.push('"');
                    let mut j = i + 1;
                    let mut terminated = false;
                    while j < len {
                        let sc = chars[j];
                        if sc == '\\' && j + 1 < len {
                            s.push(sc);
                            s.push(chars[j + 1]);
                            j += 2;
                            continue;
                        }
                        if sc == '"' {
                            s.push('"');
                            j += 1;
                            terminated = true;
                            break;
                        }
                        s.push(sc);
                        j += 1;
                    }

                    if !terminated {
                        // Unterminated string — color to end of line, mark in_string
                        spans.push(Span::styled(s, Style::default().fg(STR_COLOR)));
                        in_string = true;
                        i = len;
                        continue;
                    }

                    // Check if this string is a JSON key: skip whitespace after closing quote, look for ':'
                    let is_key = {
                        let mut k = j;
                        while k < len && chars[k].is_ascii_whitespace() {
                            k += 1;
                        }
                        k < len && chars[k] == ':'
                    };

                    let color = if is_key {
                        key_color(depth)
                    } else {
                        STR_COLOR
                    };
                    spans.push(Span::styled(s, Style::default().fg(color)));
                    i = j;
                }
                '{' | '[' => {
                    flush!();
                    spans.push(Span::styled(
                        c.to_string(),
                        Style::default().fg(brace_color(depth)),
                    ));
                    depth += 1;
                    i += 1;
                }
                '}' | ']' => {
                    flush!();
                    depth = depth.saturating_sub(1);
                    spans.push(Span::styled(
                        c.to_string(),
                        Style::default().fg(brace_color(depth)),
                    ));
                    i += 1;
                }
                ':' | ',' => {
                    flush!();
                    spans.push(Span::styled(
                        c.to_string(),
                        Style::default().fg(COMMA_COLOR),
                    ));
                    i += 1;
                }
                't' if matches!(
                    chars.get(i..i + 4),
                    Some(&['t', 'r', 'u', 'e'])
                ) =>
                {
                    let after = chars.get(i + 4).copied().unwrap_or(' ');
                    if !after.is_alphanumeric() && after != '_' {
                        flush!();
                        spans.push(Span::styled("true", Style::default().fg(BOOL_COLOR)));
                        i += 4;
                    } else {
                        buf.push(c);
                        buf_style = default_style;
                        i += 1;
                    }
                }
                'f' if matches!(
                    chars.get(i..i + 5),
                    Some(&['f', 'a', 'l', 's', 'e'])
                ) =>
                {
                    let after = chars.get(i + 5).copied().unwrap_or(' ');
                    if !after.is_alphanumeric() && after != '_' {
                        flush!();
                        spans.push(Span::styled("false", Style::default().fg(BOOL_COLOR)));
                        i += 5;
                    } else {
                        buf.push(c);
                        buf_style = default_style;
                        i += 1;
                    }
                }
                'n' if matches!(
                    chars.get(i..i + 4),
                    Some(&['n', 'u', 'l', 'l'])
                ) =>
                {
                    let after = chars.get(i + 4).copied().unwrap_or(' ');
                    if !after.is_alphanumeric() && after != '_' {
                        flush!();
                        spans.push(Span::styled(
                            "null",
                            Style::default()
                                .fg(NULL_COLOR)
                                .add_modifier(Modifier::ITALIC),
                        ));
                        i += 4;
                    } else {
                        buf.push(c);
                        buf_style = default_style;
                        i += 1;
                    }
                }
                '0'..='9' | '-' if {
                    // Start of a number: digit, or '-' followed by digit
                    c.is_ascii_digit()
                        || (c == '-' && i + 1 < len && chars[i + 1].is_ascii_digit())
                } =>
                {
                    flush!();
                    let mut num = String::new();
                    num.push(c);
                    let mut j = i + 1;
                    while j < len
                        && (chars[j].is_ascii_digit()
                            || chars[j] == '.'
                            || chars[j] == 'e'
                            || chars[j] == 'E'
                            || chars[j] == '+'
                            || chars[j] == '-')
                    {
                        num.push(chars[j]);
                        j += 1;
                    }
                    spans.push(Span::styled(num, Style::default().fg(NUM_COLOR)));
                    i = j;
                }
                ' ' | '\t' | '\r' => {
                    // Whitespace: accumulate in default style
                    if buf_style != default_style {
                        flush!();
                        buf_style = default_style;
                    }
                    buf.push(c);
                    i += 1;
                }
                _ => {
                    // Any other character: accumulate in default style
                    if buf_style != default_style {
                        flush!();
                        buf_style = default_style;
                    }
                    buf.push(c);
                    i += 1;
                }
            }
        }

        flush!();
        result.push(Line::from(spans));
    }

    result
}
