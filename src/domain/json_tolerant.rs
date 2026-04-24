//! Tolerant JSON-ish parser engine used by [`structured_parser`].
//!
//! Extracted from `structured_parser.rs` in Phase 3 DOM-008 to keep each
//! file under the green-zone line budget (spec §5.5). The [`Parser`]
//! below is the actual byte-level engine; `structured_parser` wraps it in
//! two domain-oriented entry points (`parse_whole`, `find_and_parse`).
//!
//! The engine accepts:
//! - Strict JSON objects and arrays
//! - Dart `Map.toString()` style: unquoted keys, unquoted string values,
//!   embedded commas (resolved via lookahead — see `parse_bare_value`).

use serde_json::{Map, Number, Value};

pub(super) struct Parser<'a> {
    src: &'a [u8],
    pub(super) pos: usize,
}

impl<'a> Parser<'a> {
    pub(super) fn new(s: &'a str) -> Self {
        Parser {
            src: s.as_bytes(),
            pos: 0,
        }
    }

    pub(super) fn src_len(&self) -> usize {
        self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    pub(super) fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, b: u8) -> Option<()> {
        self.skip_whitespace();
        if self.peek() == Some(b) {
            self.pos += 1;
            Some(())
        } else {
            None
        }
    }

    pub(super) fn parse_value(&mut self) -> Option<Value> {
        self.skip_whitespace();
        match self.peek()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            _ => None,
        }
    }

    fn parse_object(&mut self) -> Option<Value> {
        self.expect(b'{')?;
        let mut map = Map::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Some(Value::Object(map));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_key()?;
            self.expect(b':')?;
            let value = self.parse_entry_value(true)?;
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek()? {
                b',' => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(b'}') {
                        self.pos += 1;
                        return Some(Value::Object(map));
                    }
                }
                b'}' => {
                    self.pos += 1;
                    return Some(Value::Object(map));
                }
                _ => return None,
            }
        }
    }

    fn parse_array(&mut self) -> Option<Value> {
        self.expect(b'[')?;
        let mut arr = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Some(Value::Array(arr));
        }
        loop {
            let value = self.parse_entry_value(false)?;
            arr.push(value);
            self.skip_whitespace();
            match self.peek()? {
                b',' => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(b']') {
                        self.pos += 1;
                        return Some(Value::Array(arr));
                    }
                }
                b']' => {
                    self.pos += 1;
                    return Some(Value::Array(arr));
                }
                _ => return None,
            }
        }
    }

    /// Parse an object key. Supports quoted (`"foo"`) and unquoted
    /// (Dart identifier chars + `.` and `-`) keys.
    fn parse_key(&mut self) -> Option<String> {
        self.skip_whitespace();
        if self.peek() == Some(b'"') {
            return self.parse_quoted_string();
        }
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b':' || b.is_ascii_whitespace() {
                break;
            }
            if !is_key_char(b) {
                return None;
            }
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        Some(
            std::str::from_utf8(&self.src[start..self.pos])
                .ok()?
                .to_string(),
        )
    }

    /// Parse a value inside an object entry or array element.
    ///
    /// `in_object` controls comma-handling for bare values: inside objects
    /// the comma is ambiguous (may be inside a string value), so we use
    /// lookahead to decide; inside arrays the comma is always a separator.
    fn parse_entry_value(&mut self, in_object: bool) -> Option<Value> {
        self.skip_whitespace();
        match self.peek()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_quoted_string().map(Value::String),
            _ => self.parse_bare_value(in_object),
        }
    }

    /// Parse a bare value up to the next separator. `}` and `]` always
    /// terminate (they mean the enclosing container is closing). `,` is
    /// ambiguous in Dart Map output because string values can contain
    /// commas — but only inside objects. Inside arrays, `,` always
    /// separates elements.
    ///
    /// When `in_object` is true, we resolve `,` via lookahead: treat it
    /// as a separator only if what follows looks like a new `<key>:`
    /// entry. Otherwise fold the `,` into the bare value.
    ///
    /// Recognizes `null`, `true`, `false`, integers, floats. Anything
    /// else becomes a trimmed `Value::String`. Empty bare value
    /// (`title: ,`) → `Value::String("")`.
    fn parse_bare_value(&mut self, in_object: bool) -> Option<Value> {
        let start = self.pos;
        loop {
            match self.peek() {
                Some(b'}') | Some(b']') | None => break,
                Some(b',') => {
                    if !in_object || self.comma_is_separator() {
                        break;
                    }
                    // Object context, lookahead says not a separator.
                    self.pos += 1;
                }
                Some(_) => {
                    self.pos += 1;
                }
            }
        }
        let raw = std::str::from_utf8(&self.src[start..self.pos]).ok()?.trim();
        Some(classify_bare(raw))
    }

    /// Peek past the `,` at `self.pos`: does what follows look like
    /// `<key-chars>+\s*:` where the colon isn't the first char of `::`?
    /// Doesn't mutate `self.pos`.
    fn comma_is_separator(&self) -> bool {
        let mut i = self.pos + 1; // skip the comma
        while i < self.src.len() && self.src[i].is_ascii_whitespace() {
            i += 1;
        }
        let key_start = i;
        while i < self.src.len() && is_key_char(self.src[i]) {
            i += 1;
        }
        if i == key_start {
            return false;
        }
        while i < self.src.len() && self.src[i].is_ascii_whitespace() {
            i += 1;
        }
        if self.src.get(i).copied() != Some(b':') {
            return false;
        }
        // Avoid URL-ish `::` or `://` being mistaken for a key colon.
        if self.src.get(i + 1).copied() == Some(b':') || self.src.get(i + 1).copied() == Some(b'/')
        {
            return false;
        }
        true
    }

    /// Parse a JSON-style quoted string with `\` escape handling.
    fn parse_quoted_string(&mut self) -> Option<String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let b = self.bump()?;
            match b {
                b'"' => return Some(out),
                b'\\' => {
                    let esc = self.bump()?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        _ => {
                            // Unknown escape — keep literal.
                            out.push('\\');
                            out.push(esc as char);
                        }
                    }
                }
                _ => out.push(b as char),
            }
        }
    }
}

fn is_key_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b'.' | b'-')
}

fn classify_bare(raw: &str) -> Value {
    match raw {
        "null" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(n) = raw.parse::<i64>() {
                return Value::Number(Number::from(n));
            }
            if let Ok(n) = raw.parse::<u64>() {
                return Value::Number(Number::from(n));
            }
            if let Ok(f) = raw.parse::<f64>() {
                if let Some(n) = Number::from_f64(f) {
                    return Value::Number(n);
                }
            }
            Value::String(raw.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Driver tests for the byte-level Parser. These exercise the engine in
    // isolation from the higher-level entry points; the cross-cutting
    // scenarios live with `parse_whole` / `find_and_parse` in
    // `structured_parser.rs`.

    #[test]
    fn parser_new_starts_at_zero() {
        let p = Parser::new("abc");
        assert_eq!(p.pos, 0);
        assert_eq!(p.src_len(), 3);
    }

    #[test]
    fn parser_parses_empty_object() {
        let mut p = Parser::new("{}");
        let v = p.parse_value().unwrap();
        assert!(matches!(v, Value::Object(_)));
        assert_eq!(p.pos, 2);
    }

    #[test]
    fn parser_parses_empty_array() {
        let mut p = Parser::new("[]");
        let v = p.parse_value().unwrap();
        assert!(matches!(v, Value::Array(_)));
    }

    #[test]
    fn parser_skip_whitespace_moves_past_spaces_tabs_newlines() {
        let mut p = Parser::new("  \t\n{}");
        p.skip_whitespace();
        assert_eq!(p.pos, 4);
    }

    #[test]
    fn parser_rejects_non_container_at_root() {
        let mut p = Parser::new("42");
        assert!(p.parse_value().is_none());
    }

    #[test]
    fn classify_bare_null_true_false() {
        assert_eq!(classify_bare("null"), Value::Null);
        assert_eq!(classify_bare("true"), Value::Bool(true));
        assert_eq!(classify_bare("false"), Value::Bool(false));
    }

    #[test]
    fn classify_bare_integer_and_float() {
        assert_eq!(classify_bare("42"), Value::Number(Number::from(42)));
        let f = classify_bare("3.14");
        assert!(matches!(f, Value::Number(_)));
    }

    #[test]
    fn classify_bare_string_fallback() {
        assert_eq!(classify_bare("hello"), Value::String("hello".into()));
    }

    #[test]
    fn is_key_char_accepts_identifier_chars() {
        assert!(is_key_char(b'a'));
        assert!(is_key_char(b'Z'));
        assert!(is_key_char(b'0'));
        assert!(is_key_char(b'_'));
        assert!(is_key_char(b'$'));
        assert!(is_key_char(b'.'));
        assert!(is_key_char(b'-'));
        assert!(!is_key_char(b' '));
        assert!(!is_key_char(b':'));
        assert!(!is_key_char(b','));
    }
}
