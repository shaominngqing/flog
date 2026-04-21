# Structured Parser — Text → Value, Tolerating Dart Map.toString Output

**Date**: 2026-04-21
**Status**: Design for user review.

## Problem

After the JSON viewer rewrite landed, the logs detail panel lost its ability to format **dumped-Dart-Map messages** like:

```
{code: 0, message: ok, data: {id: 928, userId: 204394584}, items: [1, 2, 3]}
```

This is what `Map.toString()` / `jsonEncode(obj, toEncodable: _)` / `print(someMap)` produces in Dart — **keys unquoted**, **string values unquoted**, commas and colons otherwise JSON-like. It's not valid JSON, so `serde_json::from_str` rejects it and the logs panel falls back to plain `wrap_text`.

The old `bracket_format` implementation string-rewrote anything containing `{` or `[` regardless of whether it was valid JSON — which is exactly why it worked on Dart dumps but also why its depth tracking was fragile. The rewrite correctly dropped that fragile approach but didn't replace the "Dart dump" capability.

## Design

Introduce a new `domain/structured_parser.rs` module that answers one question: **given arbitrary text, can we extract a structured value out of it?** Returns `Option<serde_json::Value>`. Tries strict JSON first, falls back to a tolerant Dart-Map parser. Callers no longer have to care which format hit.

`json_viewer::tree` exposes a new `Tree::from_value(&Value)` constructor; its existing `parse(text)` is rewritten in terms of `serde_json::from_str` + `Tree::from_value`, preserving the existing behavior.

The logs and network detail callers switch to the new pipeline:

```rust
match structured_parser::parse(&msg) {
    Some(value) => {
        let tree = json_viewer::Tree::from_value(&value);
        // same rendering as today
    }
    None => { /* wrap_text fallback */ }
}
```

## Architecture

### Module layout

```
src/
├── domain/
│   └── structured_parser.rs    — NEW: text → Value
└── ui/json_viewer/
    ├── tree.rs                  — existing; refactor internals to split parse into:
    │                              (a) Tree::from_value(&Value) — pure flatten
    │                              (b) parse(text) — serde_json::from_str then from_value
    └── (other files unchanged)
```

Why `domain/`:
- Architectural rule (`CLAUDE.md`): `domain/` holds zero-UI-dependency data types and their transforms. `parser/` next to it already does "log line → LogEntry" — `structured_parser` is the same shape ("arbitrary text → Value") and sits beside it well.
- `json_viewer` stays narrowly focused: "structured data → interactive tree view." No format-sniffing in the UI layer.

### Public API of `structured_parser`

```rust
/// Best-effort parse of text that may contain a structured value.
/// Tries strict JSON first, then tolerant Dart-Map format. Returns `None`
/// if neither produces a value.
pub fn parse(text: &str) -> Option<serde_json::Value>;
```

Only one public function. The tolerant branch is private — implementation detail.

### What the tolerant branch accepts

The tolerant parser targets the shape that Dart's default `toString` produces:

- **Keys** without quotes: `{foo: 1}` → `{"foo": 1}`. Valid key chars: letters, digits, `_`, `$` (Dart identifier rules), plus `.` and `-` for lenience.
- **String values** without quotes: `{foo: hello world}` → `{"foo": "hello world"}`. A bare value is everything up to the next `,` / `}` / `]` at the same nesting level, trimmed.
- **Quoted string values** still work: `{foo: "quoted, with comma"}` → `{"foo": "quoted, with comma"}`.
- **Nested objects/arrays** recurse normally.
- **Primitives** detected in bare values: `null`, `true`, `false`, integers, floats → emitted as typed `Value`; anything else → `Value::String`.
- **Empty containers**: `{}`, `[]`.
- **Leading / trailing whitespace** ignored.

It does **NOT** try to handle:
- Escaped backslashes inside bare strings (would require a real lexer).
- Multi-line Dart objects with nested `toString()` markers like `Instance of 'ClassName'`.
- Unbalanced brackets (returns `None`).

If the tolerant parser has to make a judgment call that feels wrong, it returns `None` and the caller falls back to plain text — **better to show raw text than to mis-structure it**.

### Prefix detection

Log messages often embed structured data after a prefix: `Response: {code: 0, …}`. The tolerant parser locates the **first** `{` or `[` in the text; everything before it that isn't just whitespace is discarded for parsing purposes (it would reappear in the caller's log line anyway — the detail panel shows the full message header separately). If no opening bracket exists, the parser returns `None`.

### Tree::from_value refactor

Currently `tree.rs::parse(text)` calls `serde_json::from_str` and then `build(&value, …)` in one go. Split:

```rust
pub fn from_value(value: &serde_json::Value) -> Tree {
    let mut nodes = Vec::new();
    build(value, None, None, 0, &mut nodes);
    Tree { nodes }
}

pub fn parse(text: &str) -> Result<Tree, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    Ok(Self::from_value(&value))
}
```

Existing `tree::parse` callers and its seven tests keep working unchanged. `from_value` is the new entry point for the logs/network paths.

### Caller integration

**`src/ui/logs/detail.rs`**: replace the current `json_viewer::parse(&full_msg)` call with a two-step:

```rust
match crate::domain::structured_parser::parse(&full_msg) {
    Some(value) => {
        let tree = json_viewer::Tree::from_value(&value);
        // ... same state+render as today
    }
    None => { /* wrap_text fallback, same as today */ }
}
```

**`src/ui/network/detail.rs`** `render_json_section`: same swap. The network side almost never sees Dart dumps (Dio reports JSON strings), but applying the same code path is free and keeps the two detail panels symmetric.

### Performance

Tolerant parse runs only when `serde_json::from_str` fails — cheap fallback for the common "real JSON" case. Tolerant itself is a single pass over the characters with a small state machine (depth + in-string flag), proportional to input size. For typical log messages (<10 KB) this is nanoseconds.

### Tests

`structured_parser` owns its own test module. Coverage:

- Strict JSON still parses (delegates to serde_json).
- `{code: 0, message: ok}` → `{"code": 0, "message": "ok"}`.
- Nested: `{user: {id: 1, name: alice}}`.
- Array: `{items: [1, 2, 3]}`.
- Empty: `{}`, `[]`.
- Mixed quoted/unquoted values: `{msg: "has, comma", count: 5}`.
- Prefix handling: `Response: {code: 0}` parses the object.
- Gibberish / unbalanced returns `None`: `not structured`, `{unclosed`.
- Dart `null` / `true` / `false` literals detected as typed values.
- Whitespace tolerance: `{ foo :  bar  ,  baz : 2 }`.

The viewer tests (`tree.rs`, `state.rs`, `render.rs`) remain unchanged — this change doesn't touch the tree structure or rendering.

## Files changed

- `src/domain/mod.rs` — add `pub mod structured_parser;`
- `src/domain/structured_parser.rs` — NEW
- `src/ui/json_viewer/tree.rs` — refactor `parse` to delegate; add `from_value`
- `src/ui/json_viewer/mod.rs` — re-export `Tree::from_value` (implicit via `pub use tree::Tree`)
- `src/ui/logs/detail.rs` — swap `json_viewer::parse` call for `structured_parser::parse` + `Tree::from_value`
- `src/ui/network/detail.rs::render_json_section` — same swap

## What this does NOT include

- Editing/copying the parsed value (existing behavior unchanged).
- Detecting embedded JSON inside longer prose (`The response was {…} and it took 20ms` — the first-bracket heuristic grabs the `{`, but everything after `}` is dropped. Acceptable since the caller shows the full message in the panel header.)
- Handling Dart's `Instance of 'ClassName'` markers — those render as plain text leaves.
- Round-tripping (we only go one way: text → Value).

## Verification plan

- Unit tests in `structured_parser`: 12 cases as above.
- Manual: the screenshot-equivalent scenario with a Dart `print({code: 0, message: ok, items: [...]})`. Confirm the logs detail panel shows a proper tree.
- Manual regression: the network side still displays JSON responses identically to before.
