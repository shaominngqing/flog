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
//! then fall back to the tolerant [`Parser`] engine in
//! [`super::json_tolerant`], which accepts Dart `Map.toString()` output
//! (unquoted keys, unquoted string values, comma-in-string via lookahead
//! heuristic). Phase 3 DOM-008 extracted the engine into `json_tolerant`;
//! this file owns the high-level find/whole strategies.

use super::json_tolerant::Parser;
use serde_json::Value;

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
    if p.pos != p.src_len() {
        return None;
    }
    let _ = leading_ws;
    Some(v)
}

/// Locate a structured region embedded in `text`. Returns
/// `(start_byte, end_byte, value)` where `text[..start]` is free-form
/// prefix and `text[end..]` is free-form suffix.
///
/// Scans every `{` / `[` candidate, attempts strict JSON then tolerant
/// parsing at each, and returns the candidate with the **largest span**
/// that also passes a usefulness check (see `is_useful_match`). This
/// filters out log-tag false positives like `[DEBUG]` that parse
/// technically but have no tree-worthy structure.
pub fn find_and_parse(text: &str) -> Option<(usize, usize, Value)> {
    let mut best: Option<(usize, usize, Value)> = None;
    let mut search_from = 0usize;
    while let Some(rel) = text[search_from..].find(['{', '[']) {
        let start = search_from + rel;
        let payload = &text[start..];

        // Strict JSON first, tolerant fallback second.
        let candidate = {
            let mut de = serde_json::Deserializer::from_str(payload).into_iter::<Value>();
            match de.next() {
                Some(Ok(v)) => Some((de.byte_offset(), v)),
                _ => {
                    let mut p = Parser::new(payload);
                    p.parse_value().map(|v| (p.pos, v))
                }
            }
        };

        if let Some((consumed, v)) = candidate {
            let end = start + consumed;
            if is_useful_match(&v, start, end, text) {
                let span = consumed;
                let is_better = match &best {
                    None => true,
                    Some((bs, be, _)) => span > (*be - *bs),
                };
                if is_better {
                    best = Some((start, end, v));
                }
            }
        }

        search_from = start + 1;
    }
    best
}

/// A match is "useful" (worth rendering as a tree) unless it looks like
/// a log-level tag — a single-primitive-element array with real text
/// around it (e.g. `[DEBUG] some log...`). Everything else is accepted,
/// including small objects like `{userId: 123}`.
fn is_useful_match(v: &Value, start: usize, end: usize, text: &str) -> bool {
    if let Value::Array(arr) = v {
        if arr.len() == 1 && !is_container(&arr[0]) {
            let prefix_has_text = !text[..start].trim().is_empty();
            let suffix_has_text = !text[end..].trim().is_empty();
            if prefix_has_text || suffix_has_text {
                return false;
            }
        }
    }
    true
}

fn is_container(v: &Value) -> bool {
    matches!(v, Value::Array(_) | Value::Object(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

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
        let v = parse_str("{a: null, b: true, c: false, d: 3.25}");
        assert!(v["a"].is_null());
        assert_eq!(v["b"], true);
        assert_eq!(v["c"], false);
        assert!((v["d"].as_f64().unwrap() - 3.25).abs() < 1e-9);
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
        assert!(
            v["result"]["items"].is_array(),
            "items must be an array, got: {:?}",
            v["result"]["items"]
        );
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
        let v = parse_whole(
            "{skills: [Navigate airport check-in, Ask for directions, Handle hotel check-in]}",
        )
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

    // ── Scoring / selection tests ──────────────────────────────────────

    #[test]
    fn find_prefers_larger_region_over_tag_like_prefix() {
        // Bug: `[DEBUG]` hijacks the search because it parses as a 1-element
        // array. Real structured content is the Dart Map after it.
        let s = "[DEBUG] body: {code: 0, message: ok, items: [1, 2, 3]}";
        let (start, end, v) = find_and_parse(s).expect("should find the Dart Map, not [DEBUG]");
        assert_eq!(
            &s[start..end],
            "{code: 0, message: ok, items: [1, 2, 3]}",
            "should pick the larger region"
        );
        assert_eq!(v["code"], 0);
        assert_eq!(v["items"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn find_short_log_tag_alone_returns_none() {
        // A bare `[DEBUG]` with nothing structured after it should return
        // None — we prefer falling back to plain text over extracting a
        // useless 1-element array.
        assert!(find_and_parse("[DEBUG] some plain text").is_none());
    }

    #[test]
    fn find_real_array_not_rejected() {
        // Conversely, when the message really is an array, we should still
        // return it — don't over-reject.
        let (start, end, v) = find_and_parse("result: [1, 2, 3]").unwrap();
        assert_eq!(&v[0], 1);
        assert_eq!(&v[2], 3);
        assert_eq!(&"result: [1, 2, 3]"[start..end], "[1, 2, 3]");
    }

    // ── Dart Map containing embedded strict-JSON value ─────────────────

    #[test]
    fn dart_map_with_embedded_strict_json_object() {
        // Screenshot case: in a Dart Map the value of `content` is a strict
        // JSON literal (with quoted keys).
        let input = r#"{messageId: abc, role: assistant, content: {"goals": [{"title": "Restaurant Ordering", "icon": "✈"}]}, contentType: text}"#;
        let v = parse_whole(input).expect("should parse Dart Map with embedded JSON value");
        assert_eq!(v["messageId"], "abc");
        assert_eq!(v["content"]["goals"][0]["title"], "Restaurant Ordering");
        assert_eq!(v["contentType"], "text");
    }
}
