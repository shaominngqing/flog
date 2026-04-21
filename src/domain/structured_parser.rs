//! Tolerant text-to-Value parser.
//!
//! Given arbitrary text, try to extract a `serde_json::Value`:
//!   1. Strict JSON via `serde_json::from_str`.
//!   2. Fallback: tolerant Dart `Map.toString()` format
//!      (unquoted keys, unquoted string values).
//!
//! Returns `None` if neither produces a value — caller falls back to
//! plain text rendering.
//!
//! Example tolerant inputs:
//!   `{code: 0, message: ok}` → `{"code": 0, "message": "ok"}`
//!   `{user: {id: 1, name: alice}, tags: [a, b, c]}`
//!
//! If the text embeds a structured value after a prefix
//! (e.g. `Response: {…}`), the first `{` or `[` in the text is the
//! start of the structured region.

use serde_json::{Map, Number, Value};

/// Best-effort parse. See module doc.
pub fn parse(text: &str) -> Option<Value> {
    let start = text.find(['{', '['])?;
    let payload = &text[start..];

    // 1. Strict JSON first.
    if let Ok(v) = serde_json::from_str::<Value>(payload) {
        return Some(v);
    }

    // 2. Tolerant fallback.
    let mut p = Parser::new(payload);
    let v = p.parse_value()?;
    p.skip_whitespace();
    if p.pos != p.src.len() {
        return None;
    }
    Some(v)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Self {
        Parser {
            src: s.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_whitespace(&mut self) {
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

    fn parse_value(&mut self) -> Option<Value> {
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
            let value = self.parse_entry_value()?;
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
            let value = self.parse_entry_value()?;
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

    /// Parse a value inside an object entry or array element. Delegates
    /// to nested object/array parsing or to `parse_bare_value`.
    fn parse_entry_value(&mut self) -> Option<Value> {
        self.skip_whitespace();
        match self.peek()? {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_quoted_string().map(Value::String),
            _ => self.parse_bare_value(),
        }
    }

    /// Parse a bare value up to the next `,` / `}` / `]` at the current
    /// nesting level. Recognizes `null`, `true`, `false`, integers, floats.
    /// Anything else becomes a trimmed `Value::String`.
    fn parse_bare_value(&mut self) -> Option<Value> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if matches!(b, b',' | b'}' | b']') {
                break;
            }
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.src[start..self.pos]).ok()?.trim();
        if raw.is_empty() {
            return None;
        }
        Some(classify_bare(raw))
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

    fn parse_str(s: &str) -> Value {
        parse(s).unwrap_or_else(|| panic!("parse failed for: {:?}", s))
    }

    #[test]
    fn strict_json_still_parses() {
        let v = parse_str(r#"{"code": 0, "message": "ok"}"#);
        assert_eq!(v["code"], 0);
        assert_eq!(v["message"], "ok");
    }

    #[test]
    fn dart_map_unquoted_keys_and_strings() {
        let v = parse_str("{code: 0, message: ok}");
        assert_eq!(v["code"], 0);
        assert_eq!(v["message"], "ok");
    }

    #[test]
    fn nested_dart_map() {
        let v = parse_str("{user: {id: 1, name: alice}}");
        assert_eq!(v["user"]["id"], 1);
        assert_eq!(v["user"]["name"], "alice");
    }

    #[test]
    fn dart_array() {
        let v = parse_str("{items: [1, 2, 3]}");
        assert_eq!(v["items"][0], 1);
        assert_eq!(v["items"][2], 3);
    }

    #[test]
    fn empty_containers() {
        assert_eq!(parse_str("{}"), Value::Object(Map::new()));
        assert_eq!(parse_str("[]"), Value::Array(vec![]));
    }

    #[test]
    fn mixed_quoted_and_bare() {
        let v = parse_str(r#"{msg: "has, comma", count: 5}"#);
        assert_eq!(v["msg"], "has, comma");
        assert_eq!(v["count"], 5);
    }

    #[test]
    fn prefix_before_object() {
        let v = parse_str("Response: {code: 0}");
        assert_eq!(v["code"], 0);
    }

    #[test]
    fn gibberish_returns_none() {
        assert!(parse("not structured").is_none());
    }

    #[test]
    fn unbalanced_returns_none() {
        assert!(parse("{unclosed").is_none());
    }

    #[test]
    fn typed_bare_literals() {
        let v = parse_str("{a: null, b: true, c: false, d: 3.14}");
        assert!(v["a"].is_null());
        assert_eq!(v["b"], true);
        assert_eq!(v["c"], false);
        assert!((v["d"].as_f64().unwrap() - 3.14).abs() < 1e-9);
    }

    #[test]
    fn whitespace_tolerance() {
        let v = parse_str("{ foo :  bar  ,  baz : 2 }");
        assert_eq!(v["foo"], "bar");
        assert_eq!(v["baz"], 2);
    }

    #[test]
    fn trailing_garbage_returns_none() {
        assert!(parse("{a: 1} trailing").is_none());
    }

    #[test]
    fn text_without_brackets_returns_none() {
        assert!(parse("hello world").is_none());
    }
}
