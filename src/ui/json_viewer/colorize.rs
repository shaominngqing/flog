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

#[cfg(test)]
mod tests {
    //! Phase 2.5B Task 10b — characterization tests for `colorize_json_text`.
    //!
    //! Covers every tokenizer branch of the raw-text JSON syntax highlighter:
    //! strings (incl. escapes + unterminated), braces/brackets (depth-cycled),
    //! commas/colons, keywords (true/false/null and near-misses like truely),
    //! numbers (int, float, negative, exponent), whitespace, fallthrough
    //! text, multi-line handling, and empty lines.
    //!
    //! Rule 3 — we assert on OBSERVABLE styling: the color of each span, not
    //! the full internal representation. Rule 11 UNTESTABLE:
    //!   - Exact palette colors are constants; tests reference them via the
    //!     private `palette` module (available because tests live in the
    //!     same crate).
    use super::super::palette::{
        brace_color, key_color, BOOL_COLOR, COMMA_COLOR, NULL_COLOR, NUM_COLOR, STR_COLOR,
    };
    use super::*;

    /// Collect every span's content across all lines.
    fn all_text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn colorize_empty_string_returns_single_empty_line() {
        let out = colorize_json_text("");
        // "" splits into [""] -> empty line branch pushes Line::raw("").
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].spans.len(), 0);
    }

    #[test]
    fn colorize_blank_line_passes_through() {
        let out = colorize_json_text("\n\n");
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn colorize_simple_key_value_object() {
        let out = colorize_json_text(r#"{"a":1}"#);
        // spans expected: { (brace d0), "a" (key at depth 1 — opened brace first),
        // : (comma), 1 (num), } (brace d0 — decremented before rendering)
        let line = &out[0];
        assert_eq!(line.spans[0].content, "{");
        assert_eq!(line.spans[0].style.fg, Some(brace_color(0)));
        assert_eq!(line.spans[1].content, "\"a\"");
        assert_eq!(line.spans[1].style.fg, Some(key_color(1)));
        assert_eq!(line.spans[2].content, ":");
        assert_eq!(line.spans[2].style.fg, Some(COMMA_COLOR));
        assert_eq!(line.spans[3].content, "1");
        assert_eq!(line.spans[3].style.fg, Some(NUM_COLOR));
        assert_eq!(line.spans[4].content, "}");
        assert_eq!(line.spans[4].style.fg, Some(brace_color(0)));
    }

    #[test]
    fn colorize_nested_object_uses_depth_colors() {
        let out = colorize_json_text(r#"{"a":{"b":1}}"#);
        let line = &out[0];
        // After '{' depth=1 → "a" uses key_color(1);
        // after inner '{' depth=2 → "b" uses key_color(2). Distinct.
        let key_a_fg = line
            .spans
            .iter()
            .find(|s| s.content == "\"a\"")
            .and_then(|s| s.style.fg)
            .unwrap();
        let key_b_fg = line
            .spans
            .iter()
            .find(|s| s.content == "\"b\"")
            .and_then(|s| s.style.fg)
            .unwrap();
        assert_ne!(key_a_fg, key_b_fg);
        assert_eq!(key_a_fg, key_color(1));
        assert_eq!(key_b_fg, key_color(2));
    }

    #[test]
    fn colorize_string_value_uses_str_color() {
        let out = colorize_json_text(r#""hello""#);
        // Standalone string — no colon follows, so it's a value.
        assert_eq!(out[0].spans[0].content, "\"hello\"");
        assert_eq!(out[0].spans[0].style.fg, Some(STR_COLOR));
    }

    #[test]
    fn colorize_string_with_escape() {
        let out = colorize_json_text(r#""he\"llo""#);
        assert_eq!(out[0].spans[0].content, r#""he\"llo""#);
        assert_eq!(out[0].spans[0].style.fg, Some(STR_COLOR));
    }

    #[test]
    fn colorize_unterminated_string_enters_multiline_state() {
        // Line 1 opens a string that doesn't close → line 2 continues in-string.
        let out = colorize_json_text("\"abc\n\"def\"");
        // Line 0: opened string, pushed as STR_COLOR unterminated
        assert_eq!(out[0].spans[0].style.fg, Some(STR_COLOR));
        // Line 1: continues inside string until closing '"'
        assert!(!out[1].spans.is_empty());
        assert_eq!(out[1].spans[0].style.fg, Some(STR_COLOR));
    }

    #[test]
    fn colorize_array_brackets_uses_brace_color() {
        let out = colorize_json_text(r#"[1, 2]"#);
        let line = &out[0];
        assert_eq!(line.spans[0].content, "[");
        assert_eq!(line.spans[0].style.fg, Some(brace_color(0)));
        // 1
        assert_eq!(line.spans[1].style.fg, Some(NUM_COLOR));
        // ','
        let comma_span = line.spans.iter().find(|s| s.content == ",").unwrap();
        assert_eq!(comma_span.style.fg, Some(COMMA_COLOR));
        // ']'
        let close_span = line.spans.iter().find(|s| s.content == "]").unwrap();
        assert_eq!(close_span.style.fg, Some(brace_color(0)));
    }

    #[test]
    fn colorize_true_and_false_use_bool_color() {
        let out = colorize_json_text("true false");
        let t = out[0].spans.iter().find(|s| s.content == "true").unwrap();
        let f = out[0].spans.iter().find(|s| s.content == "false").unwrap();
        assert_eq!(t.style.fg, Some(BOOL_COLOR));
        assert_eq!(f.style.fg, Some(BOOL_COLOR));
    }

    #[test]
    fn colorize_null_uses_null_color_and_italic() {
        use ratatui::style::Modifier;
        let out = colorize_json_text("null");
        let n = &out[0].spans[0];
        assert_eq!(n.content, "null");
        assert_eq!(n.style.fg, Some(NULL_COLOR));
        assert!(n.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn colorize_truely_is_not_keyword() {
        // "truely" starts with "true" but followed by alphanumeric — not recognized.
        let out = colorize_json_text("truely");
        let joined = all_text(&out);
        assert_eq!(joined, "truely");
        // No span should be styled as BOOL_COLOR for the "true" prefix.
        for span in &out[0].spans {
            assert_ne!(span.style.fg, Some(BOOL_COLOR));
        }
    }

    #[test]
    fn colorize_falsey_is_not_keyword() {
        let out = colorize_json_text("falsey");
        assert_eq!(all_text(&out), "falsey");
        for span in &out[0].spans {
            assert_ne!(span.style.fg, Some(BOOL_COLOR));
        }
    }

    #[test]
    fn colorize_nullable_is_not_keyword() {
        let out = colorize_json_text("nullable");
        assert_eq!(all_text(&out), "nullable");
        for span in &out[0].spans {
            assert_ne!(span.style.fg, Some(NULL_COLOR));
        }
    }

    #[test]
    fn colorize_integer() {
        let out = colorize_json_text("42");
        assert_eq!(out[0].spans[0].content, "42");
        assert_eq!(out[0].spans[0].style.fg, Some(NUM_COLOR));
    }

    #[test]
    fn colorize_float() {
        let out = colorize_json_text("3.14");
        assert_eq!(out[0].spans[0].content, "3.14");
        assert_eq!(out[0].spans[0].style.fg, Some(NUM_COLOR));
    }

    #[test]
    fn colorize_negative_number() {
        let out = colorize_json_text("-17");
        assert_eq!(out[0].spans[0].content, "-17");
        assert_eq!(out[0].spans[0].style.fg, Some(NUM_COLOR));
    }

    #[test]
    fn colorize_negative_not_followed_by_digit_is_text() {
        // "- abc" → the '-' goes to fallthrough (default text), not a number.
        let out = colorize_json_text("- abc");
        let joined = all_text(&out);
        assert_eq!(joined, "- abc");
        // No NUM_COLOR span.
        for span in &out[0].spans {
            assert_ne!(span.style.fg, Some(NUM_COLOR));
        }
    }

    #[test]
    fn colorize_exponent_number() {
        let out = colorize_json_text("1.5e10");
        assert_eq!(out[0].spans[0].content, "1.5e10");
        assert_eq!(out[0].spans[0].style.fg, Some(NUM_COLOR));
    }

    #[test]
    fn colorize_malformed_json_passes_through() {
        // Closing brace with no open — depth saturating_sub keeps it safe.
        let out = colorize_json_text("}]");
        assert_eq!(out[0].spans.len(), 2);
        // Both are brace-colored.
        assert_eq!(out[0].spans[0].style.fg, Some(brace_color(0)));
    }

    #[test]
    fn colorize_preserves_surrounding_non_json_text() {
        let out = colorize_json_text("prefix {\"k\":1} suffix");
        let joined = all_text(&out);
        assert_eq!(joined, "prefix {\"k\":1} suffix");
    }

    #[test]
    fn colorize_multiline_json() {
        let src = "{\n  \"a\": 1\n}";
        let out = colorize_json_text(src);
        assert_eq!(out.len(), 3);
        // line 0: '{'
        assert_eq!(out[0].spans[0].content, "{");
        // line 1 contains "a" key and a number
        let l1_text = all_text(&[out[1].clone()]);
        assert!(l1_text.contains("\"a\""));
        assert!(l1_text.contains("1"));
        // line 2: '}'
        assert_eq!(out[2].spans[0].content, "}");
    }

    #[test]
    fn colorize_whitespace_runs_grouped_as_default() {
        let out = colorize_json_text("   ");
        // three spaces flushed as a single default span on exit.
        assert_eq!(out[0].spans.len(), 1);
        assert_eq!(out[0].spans[0].content, "   ");
    }

    #[test]
    fn colorize_tab_and_cr_in_whitespace() {
        let out = colorize_json_text("\t\r ");
        assert_eq!(out[0].spans.len(), 1);
        assert_eq!(out[0].spans[0].content, "\t\r ");
    }

    #[test]
    fn colorize_unterminated_string_middle_of_line() {
        // Open a string that never closes on this single line.
        let out = colorize_json_text("\"abc");
        assert_eq!(out[0].spans[0].style.fg, Some(STR_COLOR));
    }

    #[test]
    fn colorize_escape_in_unterminated_string_on_continuing_line() {
        // Multi-line: line 0 opens with a string that goes into line 1 with an escape.
        let src = "\"a\nb\\nend\"";
        let out = colorize_json_text(src);
        // line 1 was in_string and should close after final '"'.
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn colorize_key_with_whitespace_before_colon() {
        // "a"  :1 — key detection skips whitespace before ':'
        let out = colorize_json_text(r#"{"a" :1}"#);
        let line = &out[0];
        let a_span = line.spans.iter().find(|s| s.content == "\"a\"").unwrap();
        assert_eq!(a_span.style.fg, Some(key_color(1)));
    }

    #[test]
    fn colorize_deep_nesting_beyond_palette_wraps() {
        // 7 nesting levels — palette has 6. After opening 7 '{' depth=7; key_color(7 % 6) == key_color(1).
        let out = colorize_json_text(r#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":1}}}}}}}"#);
        let line = &out[0];
        let g_span = line.spans.iter().find(|s| s.content == "\"g\"").unwrap();
        assert_eq!(g_span.style.fg, Some(key_color(7)));
    }
}
