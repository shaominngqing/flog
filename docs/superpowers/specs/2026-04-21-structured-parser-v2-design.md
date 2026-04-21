# Structured Parser v2 — Log-Aware Framing + Tolerant Comma Handling

**Date**: 2026-04-21
**Status**: Design supersedes `2026-04-21-structured-parser-design.md`.

## What Changed vs. v1

v1 collapsed "find the structured region" and "parse the structured region" into one `parse(text: &str)` call, and made `logs/detail.rs` use it. Two problems surfaced in live use:

1. **Log context gets eaten.** A log like `body: {categories: [...]}` ends up rendering only the tree — the `body:` prefix that gives the message semantic meaning is thrown away. The parser's "find first bracket, drop prefix" heuristic was fine for network bodies (which have no prefix) but wrong for log messages (which routinely carry a prefix, and sometimes a suffix too).
2. **Strings containing commas defeat the parser.** A Dart dump like `description: By completing this course, learners will be able...` breaks because `parse_bare_value` stops at the first `,`, treats `learners will be able` as the next key, finds no `:`, returns `None`, and the whole parse fails. The panel falls back to raw wrap_text.

v1's architecture put the prefix-stripping logic inside the parser, which was wrong separation of concerns. It's a *log-rendering* decision, not a *parsing* one.

## Design (v2)

### Two public entry points

The parser exposes exactly two functions, each named for what it does:

```rust
/// Parse `text` as one whole structured value. Returns None if the entire
/// input (after trimming) isn't a single object or array.
/// Used by: network/detail.rs for Response Body, Request Body, SSE chunks,
/// WS messages — all of which are already isolated structured-data strings.
pub fn parse_whole(text: &str) -> Option<Value>;

/// Locate a structured region embedded in a longer text. Returns
/// `Some((start, end, value))` where `text[..start]` is free-form prefix,
/// `text[start..end]` is the structured region, and `text[end..]` is
/// free-form suffix. Returns None if no structured region is found.
/// Used by: logs/detail.rs — log messages often carry context before
/// and after the structured data.
pub fn find_and_parse(text: &str) -> Option<(usize, usize, Value)>;
```

Both funnel through the same inner tolerant parser. Separating the two caller shapes at the API level makes the intent explicit and stops callers from accidentally getting the wrong behavior.

### Finding the structured region

`find_and_parse` scans for a candidate start (`{` or `[`), attempts a parse from there, and on success records the byte position where the parser stopped as `end`. On failure it advances past the failed bracket and tries the next one. If no candidate produces a parse, returns None.

This means a log like `before {broken` won't hijack the rendering — we try, fail cleanly, return None, and the caller shows raw text.

### Comma-lookahead heuristic

The root cause of comma-in-string failures is that Dart's `Map.toString()` format doesn't escape commas inside string values. There's no 100%-correct algorithm (the format is ambiguous) but a simple lookahead covers the common cases:

**Rule:** when `parse_bare_value` sees a comma, peek past it. If the text after the comma (skipping whitespace) looks like `<key-chars>+:` (identifier chars followed by a colon, with optionally-balanced whitespace), treat the comma as a separator. Otherwise, treat the comma as content and continue consuming the bare value.

The `<key-chars>` regex is the same set `parse_key` already accepts (`[A-Za-z0-9_$.\-]`). We also require that after finding the colon, the next char is NOT another colon — this avoids treating URLs like `http://` inside bare strings as keys.

**What this catches:** `description: By completing this course, learners will be able...` — after the first `,` the next token is `learners`, a bareword, but reading ahead we find `learners will be able` and no `:` before the next `,` — so the lookahead fails to find `<key>:` and treats the comma as content. The description keeps growing until we hit `, level:` — `level` is followed by `:`, so that IS a separator, and we stop there.

**What this misses:** pathological cases where a string value itself contains `key: value` substrings. These remain ambiguous; we prefer matching the common case over being bulletproof.

**Nested containers:** bare values cannot contain `{` / `[` / `]` / `}` at all (those characters always end the bare-value scan regardless of heuristic). So the lookahead only affects `,`.

### Integration changes

**`src/ui/logs/detail.rs`** switches from `structured_parser::parse` to `find_and_parse`. On success, the panel emits three sections:

1. **Prefix** — `text[..start]` rendered as plain wrapped text in the default TEXT color.
2. **Tree** — rendered via the existing `Tree::from_value(&value)` + `append_render` pipeline.
3. **Suffix** — `text[end..]` rendered as plain wrapped text (only if non-empty after trim).

On failure — entire message as plain wrapped text, same as today.

Click-map and scroll handling adjust for the new prefix/suffix lines. Specifically: `viewer_click_map` only covers the tree rows; prefix and suffix rows map to None.

The existing fingerprint-based fold-state reset keeps working — it hashes the whole `full_msg`, which already captures any change to prefix/suffix/tree.

**`src/ui/network/detail.rs::render_json_section`** switches to `parse_whole`. Response/Request bodies are pre-framed by the HTTP layer; there's no prefix to worry about. If `parse_whole` fails, fall back to `wrap_text` as today.

### What stays the same

- `json_viewer` module is untouched.
- `Tree::from_value` contract unchanged.
- All existing parser test cases continue to pass with the new entry points (we add a `parse` helper in tests that calls `parse_whole` and retains the old test shape).

## Files changed

- `src/domain/structured_parser.rs` — rename `parse` to `parse_whole`; add `find_and_parse`; modify `parse_bare_value` to use comma-lookahead.
- `src/ui/logs/detail.rs` — switch to `find_and_parse`; render prefix + tree + suffix.
- `src/ui/network/detail.rs` — switch to `parse_whole`.

## Test plan

New test cases in `structured_parser`:

- `comma_inside_bare_string_value` — `{a: 1, b: description with, commas inside}` → `b = "description with, commas inside"`.
- `comma_followed_by_key_is_separator` — `{a: hello, b: 1}` → `a = "hello"`, `b = 1`.
- `url_in_bare_value_not_mistaken_for_key` — `{a: http://example.com/x, b: 2}` → `a = "http://example.com/x"`, `b = 2`.
- `find_in_log_message_with_prefix_and_suffix` — `find_and_parse("body: {a: 1} took 12ms")` → `Some((6, 12, {"a": 1}))`.
- `find_returns_none_for_plain_text` — `find_and_parse("Application started in 120ms")` → `None`.
- `find_skips_unparseable_brackets` — `find_and_parse("before {broken but {a: 1} here")` → finds the second one.
- `parse_whole_rejects_trailing_junk` — preserved v1 behavior: `parse_whole("{a: 1} junk")` → `None`.
- `description_from_screenshot_parses` — full sample with comma-containing `description` field ends cleanly and the `description` value is intact.

Logs/Network caller changes covered by existing manual verification (reinstalling flog and re-running against the flutter app).

## Non-goals

- Multiple structured regions in one message (`Req {a: 1} and {b: 2}`): only the **first** parseable region is extracted. The rest becomes suffix text. If this turns out to be common, revisit — but the cost of iterating and merging multiple trees is not worth it for the prevalence.
- Multi-line prefixes wrapping through the terminal in unusual ways: prefix rendering reuses the existing `wrap_text` helper, same behavior as the fallback today.
- Escape sequences inside bare values — Dart doesn't use them in `toString()` output.
