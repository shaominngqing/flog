//! Raw JSON text syntax highlighter.
//!
//! Character-by-character tokenizer that colorizes arbitrary text containing
//! JSON-like fragments. Tolerates any input — unterminated strings, missing
//! brackets, mid-edit gibberish — and keeps colorizing through errors.
//!
//! Used by the Mock rule body editor (`ui/network/mock_rules.rs`) to live-
//! highlight user input while they type. Shares the palette with the AST-based
//! tree viewer so the two look visually consistent.
//!
//! Not the tree viewer — that one lives in `tree.rs` / `render.rs`, requires
//! valid JSON, and supports folding. Use it for display-only JSON.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::super::TEXT;
use super::palette::{
    brace_color, key_color, BOOL_COLOR, COMMA_COLOR, NULL_COLOR, NUM_COLOR, STR_COLOR,
};

/// Colorize raw text (typically JSON) with syntax highlighting.
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

        macro_rules! flush {
            () => {
                if !buf.is_empty() {
                    spans.push(Span::styled(std::mem::take(&mut buf), buf_style));
                }
            };
        }

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
                        spans.push(Span::styled(s, Style::default().fg(STR_COLOR)));
                        in_string = true;
                        i = len;
                        continue;
                    }
                    let is_key = {
                        let mut k = j;
                        while k < len && chars[k].is_ascii_whitespace() {
                            k += 1;
                        }
                        k < len && chars[k] == ':'
                    };
                    let color = if is_key { key_color(depth) } else { STR_COLOR };
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
                't' if matches!(chars.get(i..i + 4), Some(&['t', 'r', 'u', 'e'])) => {
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
                'f' if matches!(chars.get(i..i + 5), Some(&['f', 'a', 'l', 's', 'e'])) => {
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
                'n' if matches!(chars.get(i..i + 4), Some(&['n', 'u', 'l', 'l'])) => {
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
                '0'..='9' | '-'
                    if {
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
                    if buf_style != default_style {
                        flush!();
                        buf_style = default_style;
                    }
                    buf.push(c);
                    i += 1;
                }
                _ => {
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
