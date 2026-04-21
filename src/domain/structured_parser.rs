//! Tolerant text-to-Value parser.
//!
//! Two entry points tailored to different caller needs:
//!
//! - [`parse_whole`] — the entire input (after trim) must be one structured
//!   value. Used by network Response/Request body rendering, where the
//!   input is already an isolated JSON string.
//!
//! - [`find_and_parse`] — locate a structured region embedded in free-form
//!   text. Returns the byte range plus the parsed value, so callers can
//!   render the prefix and suffix around the tree. Used by log messages,
//!   which commonly read `body: {…} took 12ms` or similar.
//!
//! Both entry points first try strict JSON via `serde_json::from_str`,
//! then fall back to a tolerant parser that accepts Dart `Map.toString()`
//! output (unquoted keys, unquoted string values, comma-in-string via
//! lookahead heuristic — see `parse_bare_value`).

use serde_json::{Map, Number, Value};

/// Parse the entire input as a single structured value. Leading and
/// trailing whitespace are tolerated; anything else after the value
/// causes failure. Returns `None` if the input isn't one structured value.
pub fn parse_whole(text: &str) -> Option<Value> {
    let trimmed = text.trim_start();
    let leading_ws = text.len() - trimmed.len();

    // 1. Strict JSON first.
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Some(v);
    }

    // 2. Tolerant fallback.
    let mut p = Parser::new(trimmed);
    let v = p.parse_value()?;
    p.skip_whitespace();
    if p.pos != p.src.len() {
        return None;
    }
    let _ = leading_ws;
    Some(v)
}

/// Locate a structured region embedded in `text`. Returns
/// `(start_byte, end_byte, value)` where `text[..start]` is free-form
/// prefix and `text[end..]` is free-form suffix.
///
/// Scans for candidate `{` or `[` starts left-to-right; tries strict
/// JSON first, then tolerant parsing. On failure at one candidate,
/// advances to the next bracket and retries.
pub fn find_and_parse(text: &str) -> Option<(usize, usize, Value)> {
    let bytes = text.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(['{', '[']) {
        let start = search_from + rel;
        let payload = &text[start..];

        // 1. Strict JSON: serde_json::StreamDeserializer lets us detect
        // how much was consumed via the Deserializer's byte offset.
        let mut de = serde_json::Deserializer::from_str(payload).into_iter::<Value>();
        if let Some(Ok(v)) = de.next() {
            let consumed = de.byte_offset();
            return Some((start, start + consumed, v));
        }

        // 2. Tolerant fallback.
        let mut p = Parser::new(payload);
        if let Some(v) = p.parse_value() {
            let consumed = p.pos;
            return Some((start, start + consumed, v));
        }

        // Advance past this bracket and try the next.
        search_from = start + 1;
        // Don't split a multi-byte boundary — `find` always returns char
        // boundaries so `start + 1` is safe for ASCII `{`/`[`.
        let _ = bytes;
    }
    None
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
        if self.src.get(i + 1).copied() == Some(b':') || self.src.get(i + 1).copied() == Some(b'/') {
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

    fn parse_str(s: &str) -> Value {
        parse_whole(s).unwrap_or_else(|| panic!("parse failed for: {:?}", s))
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
    fn find_and_parse_with_prefix() {
        let (start, end, v) = find_and_parse("Response: {code: 0}").unwrap();
        assert_eq!(start, "Response: ".len());
        assert_eq!(end, "Response: {code: 0}".len());
        assert_eq!(v["code"], 0);
    }

    #[test]
    fn gibberish_returns_none() {
        assert!(parse_whole("not structured").is_none());
        assert!(find_and_parse("not structured").is_none());
    }

    #[test]
    fn unbalanced_returns_none() {
        assert!(parse_whole("{unclosed").is_none());
        assert!(find_and_parse("{unclosed").is_none());
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
    fn parse_whole_rejects_trailing_junk() {
        assert!(parse_whole("{a: 1} trailing").is_none());
    }

    #[test]
    fn parse_whole_rejects_text_without_brackets() {
        assert!(parse_whole("hello world").is_none());
    }

    #[test]
    fn dart_map_with_nested_array_of_maps() {
        // Reproduces the screenshot scenario: response body is a Dart Map
        // whose `result.items` is a list of Dart Maps.
        let input = "body: {code: 0, message: ok, result: {items: [{id: 928, userId: 204, title: Business Meetings}, {id: 929, userId: 204, title: Travel English}], total: 2}, weeks: []}";
        let (_, _, v) = find_and_parse(input).expect("should parse");
        // Root should be an object, NOT absorb the items into itself.
        assert_eq!(v["code"], 0);
        assert_eq!(v["message"], "ok");
        assert!(v["result"]["items"].is_array(), "items must be an array, got: {:?}", v["result"]["items"]);
        let items = v["result"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "items should have 2 elements");
        assert_eq!(items[0]["id"], 928);
        assert_eq!(items[1]["id"], 929);
        assert_eq!(v["result"]["total"], 2);
        assert!(v["weeks"].is_array());
        assert_eq!(v["weeks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn dart_map_empty_string_value() {
        // Dart prints empty strings as nothing: `title: ,` — bare value is empty.
        let v = parse_whole("{id: 963, title: , level: 2}").expect("should parse");
        assert_eq!(v["id"], 963);
        assert_eq!(v["title"], "");
        assert_eq!(v["level"], 2);
    }

    #[test]
    fn dart_array_of_bare_string_phrases() {
        // Skills list from the screenshot: multi-word phrases separated by `,`.
        let v = parse_whole("{skills: [Navigate airport check-in, Ask for directions, Handle hotel check-in]}")
            .expect("should parse");
        let skills = v["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0], "Navigate airport check-in");
        assert_eq!(skills[1], "Ask for directions");
        assert_eq!(skills[2], "Handle hotel check-in");
    }

    #[test]
    fn dart_map_with_unicode_emoji_value() {
        // Dart icon fields can contain emoji: `icon: ✈`
        let v = parse_whole("{name: alice, icon: ✈, age: 30}").expect("should parse");
        assert_eq!(v["name"], "alice");
        assert_eq!(v["icon"], "✈");
        assert_eq!(v["age"], 30);
    }

    #[test]
    fn full_screenshot_response_body() {
        // A compressed version of the exact response shape from the bug report.
        let input = r#"body: {id: 963, userId: 204394584, userGoalId: 788, title: , level: 2, sceneType: 7000, courseStatus: 10, createdTime: 1776760746431, updatedTime: 1776760746466, goal: {id: 788, userId: 204394584, key: goal-mo8dh22h-mt7tlail, level: 2, weeks: 4, icon: ✈, skills: [Navigate airport check-in and security, Ask for directions and local information, Handle hotel check-in and room issues], conversationId: , createdTime: 1776760746425, updatedTime: 1776760746425}, weeks: []}"#;
        let (_, _, v) = find_and_parse(input).expect("should parse full screenshot body");
        assert_eq!(v["id"], 963);
        assert_eq!(v["title"], "");
        assert_eq!(v["goal"]["skills"].as_array().unwrap().len(), 3);
        assert_eq!(v["goal"]["icon"], "✈");
        assert_eq!(v["weeks"].as_array().unwrap().len(), 0);
    }

    // ── Comma-lookahead heuristic ──────────────────────────────────────

    #[test]
    fn comma_inside_bare_string_value() {
        // description contains commas; they should NOT split it into keys.
        let v = parse_str(
            "{a: 1, description: By completing this course, learners will be able to order, level: 2}",
        );
        assert_eq!(v["a"], 1);
        assert_eq!(
            v["description"],
            "By completing this course, learners will be able to order"
        );
        assert_eq!(v["level"], 2);
    }

    #[test]
    fn comma_followed_by_key_is_separator() {
        let v = parse_str("{a: hello, b: 1}");
        assert_eq!(v["a"], "hello");
        assert_eq!(v["b"], 1);
    }

    #[test]
    fn url_in_bare_value_not_mistaken_for_key() {
        // Although the fallback for `:` is at comma-separator level, make
        // sure a URL-like substring doesn't confuse the parser. There's
        // no comma inside the URL itself, so the lookahead for splitting
        // only kicks in at `, next: ...`.
        let v = parse_str("{url: http://example.com/x, port: 8080}");
        assert_eq!(v["url"], "http://example.com/x");
        assert_eq!(v["port"], 8080);
    }

    #[test]
    fn comma_before_url_with_double_colon() {
        // Heuristic should NOT treat `http:` as a key (the `//` check
        // rejects it). So `, http://…` after a bare value stays inside it.
        let v = parse_str("{note: see url, http://docs.example.com for more, done: true}");
        assert_eq!(v["note"], "see url, http://docs.example.com for more");
        assert_eq!(v["done"], true);
    }

    // ── find_and_parse integration ─────────────────────────────────────

    #[test]
    fn find_in_log_message_with_prefix_and_suffix() {
        let s = "body: {a: 1} took 12ms";
        let (start, end, v) = find_and_parse(s).unwrap();
        assert_eq!(&s[..start], "body: ");
        assert_eq!(&s[start..end], "{a: 1}");
        assert_eq!(&s[end..], " took 12ms");
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn find_returns_none_for_plain_text() {
        assert!(find_and_parse("Application started in 120ms").is_none());
    }

    #[test]
    fn find_skips_unparseable_brackets() {
        // First `{broken` fails; parser advances and finds `{a: 1}`.
        let s = "before {broken but {a: 1} here";
        let (start, end, v) = find_and_parse(s).unwrap();
        assert_eq!(&s[start..end], "{a: 1}");
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn find_strict_json_byte_offset_accurate() {
        let s = "result: {\"code\": 0} end";
        let (start, end, v) = find_and_parse(s).unwrap();
        assert_eq!(&s[..start], "result: ");
        assert_eq!(&s[start..end], r#"{"code": 0}"#);
        assert_eq!(&s[end..], " end");
        assert_eq!(v["code"], 0);
    }

    #[test]
    fn find_full_screenshot_body() {
        // The exact shape from the bug report, with `body: ` prefix.
        let s = r#"body: {id: 963, userId: 204394584, description: This course covers, among other things, ordering and, well, asking for help, level: 2}"#;
        let (start, end, v) = find_and_parse(s).unwrap();
        assert_eq!(&s[..start], "body: ");
        assert!(s[end..].is_empty() || s[end..].chars().all(|c| c.is_whitespace()));
        assert_eq!(v["id"], 963);
        assert_eq!(
            v["description"],
            "This course covers, among other things, ordering and, well, asking for help"
        );
        assert_eq!(v["level"], 2);
    }
}
