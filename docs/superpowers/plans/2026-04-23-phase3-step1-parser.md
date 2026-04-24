# Phase 3 Step 3.1 — Parser Layer Redesign

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **🛑 USER APPROVAL REQUIRED before executing any task.** This document is both the step design doc (spec §5.4 item b) and the execution plan. Read Sections 1-3 first; once the user confirms the design, begin with Task 0.

**Goal:** Redesign the parser layer so future parsers are injectable (DOM-013), the ANSI-strip helper is shared (DOM-015), the keyword-inference rules live as named constants (DOM-017), and generic.rs's three-deep fallthrough is flattened into helpers (DOM-016). LazyLock regex structure stays (DOM-014 is A — no change, just a module-level comment linking to the audit).

**Architecture:** One new module `src/parser/util.rs` owns `strip_ansi()` and a module-level `ANSI_RE`. `MultiStrategyParser` gets a public `with_strategies()` constructor (DOM-013). `generic.rs` extracts three private helpers (DOM-016). `keyword.rs` keyword sets move to `const KEYWORDS_*: &[&str]` with a build-regex-from-const helper (DOM-017). No file grows past 500 lines; no new dependencies.

**Tech Stack:** Rust 1.x, `regex` crate (already in Cargo.toml), `std::sync::LazyLock`. No additions.

**Parent spec:** `docs/superpowers/specs/2026-04-22-project-cleanup-design.md` §5 (Phase 3)
**Audit source:** `docs/superpowers/audit/02-domain.md` (DOM-013 D, DOM-014 A, DOM-015 D, DOM-016 A, DOM-017 D)

---

## 1. 旧设计问题 (spec §5.4 (b) requirement)

### DOM-013 — `MultiStrategyParser::default_chain()` hard-wires the strategy list

Today:
```rust
// src/parser/mod.rs
impl MultiStrategyParser {
    pub fn default_chain() -> Self {
        Self {
            strategies: vec![
                Box::new(structured::StructuredParser),
                Box::new(generic::GenericParser),
                Box::new(keyword::KeywordParser),
            ],
        }
    }
}
```

No public constructor accepts custom strategies. Callers cannot:
- inject a test fake
- add a custom parser (e.g., domain-specific formats downstream)
- reorder for A/B testing

### DOM-015 — ANSI-strip duplication

`src/parser/generic.rs:35` and `src/parser/structured.rs:17` both define:
```rust
static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());
```
Identical regex, identical call site (`ANSI_RE.replace_all(raw, "")`), two copies. Drift risk: someone updates the regex in one file but not the other.

### DOM-016 — Generic parser fallthrough is 3-deep nested if-let

`src/parser/generic.rs:88-128` (verified line numbers will shift post-Phase-2.5B — use `grep` on `FLUTTER_PLAIN_RE.captures`):
```rust
if let Some(caps) = FLUTTER_PLAIN_RE.captures(line) {
    let content = ANSI_RE.replace_all(raw, "").to_string();
    if let Some(bcaps) = BRACKET_LEVEL_RE.captures(&content) {
        // 20-line build of LogEntry for [LEVEL][Tag] inside flutter
        return Some(...);
    }
    // Plain flutter content (including empty lines from print(''))
    return Some(LogEntry { level: System, tag: "flutter", ... });
}
```
Works, but readers have to hold three conditions in their head.

### DOM-017 — Keyword regex literals bury the keyword set

`src/parser/keyword.rs:12-21`:
```rust
static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(error|exception|fatal|crash|panic|fail(ed|ure)?)\b").unwrap()
});
```
The keyword set is embedded in a regex string. Changing it means editing a regex pattern. No single source of truth for "what counts as an ERROR keyword".

### DOM-014 — LazyLock pattern is fine, just needs acknowledgement

Audit marked A with proposed_action "No change needed. LazyLock is correct." Plan honours this: **no code change, one-line comment at each LazyLock site pointing to the audit.**

---

## 2. 新设计思路

### 2.1 New file: `src/parser/util.rs`

```rust
//! Shared regex helpers for all parser strategies.

use regex::Regex;
use std::sync::LazyLock;

/// ANSI escape sequence matcher. Shared between every parser so that
/// updates (e.g., supporting OSC-8 hyperlink sequences) happen in one
/// place. Phase 3 Step 3.1 extraction — see Audit DOM-015.
pub static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

/// Strip ANSI escape sequences from a string. Returns `Cow` so callers
/// don't allocate when no escapes are present.
pub fn strip_ansi(s: &str) -> std::borrow::Cow<'_, str> {
    ANSI_RE.replace_all(s, "")
}
```

Expose via `pub mod util;` in `src/parser/mod.rs`. Call sites use
`parser::util::strip_ansi(line)` or `parser::util::ANSI_RE.replace_all(...)`.

### 2.2 `MultiStrategyParser::with_strategies`

```rust
// src/parser/mod.rs
impl MultiStrategyParser {
    /// Create a parser chain with an explicit list of strategies.
    /// Callers decide order. Useful for testing or custom parser injection.
    /// Phase 3 Step 3.1 — see Audit DOM-013.
    pub fn with_strategies(strategies: Vec<Box<dyn LogLineParser>>) -> Self {
        Self { strategies }
    }

    pub fn default_chain() -> Self {
        Self::with_strategies(vec![
            Box::new(structured::StructuredParser),
            Box::new(generic::GenericParser),
            Box::new(keyword::KeywordParser),
        ])
    }
}
```

`default_chain()` delegates to `with_strategies()` — single source of truth
for how a chain is built.

### 2.3 `generic.rs` fallthrough extraction

Split the 3-deep nested `if let` into three private helpers:

```rust
impl GenericParser {
    /// Try to parse `I/flutter (pid): ...` or `flutter: ...` style lines.
    /// Returns `Some(entry)` if the flutter prefix is recognized.
    fn try_parse_flutter_prefixed(line: &str) -> Option<LogEntry> {
        FLUTTER_PLAIN_RE.captures(line).map(|caps| {
            let content = super::util::strip_ansi(caps.get(1).unwrap().as_str()).into_owned();
            // If content starts with [LEVEL][Tag], promote it.
            Self::try_parse_flutter_structured(&content)
                .unwrap_or_else(|| Self::build_flutter_plain(content))
        })
    }

    /// If flutter content starts with `[LEVEL][Tag] msg`, parse it.
    fn try_parse_flutter_structured(content: &str) -> Option<LogEntry> {
        let caps = BRACKET_LEVEL_RE.captures(content)?;
        // Build LogEntry from caps. Same logic as current inline.
        Some(LogEntry { ... })
    }

    /// Infallible: build a System-level entry tagged "flutter".
    fn build_flutter_plain(content: String) -> LogEntry {
        LogEntry { level: LogLevel::System, tag: "flutter".into(), message: content, ... }
    }
}
```

`try_parse()` becomes:
```rust
fn try_parse(&self, line: &str) -> Option<LogEntry> {
    if let Some(entry) = Self::try_parse_flutter_prefixed(line) {
        return Some(entry);
    }
    if let Some(entry) = Self::try_parse_logcat_line(line) {
        return Some(entry);
    }
    if let Some(entry) = Self::try_parse_exception_block(line) {
        return Some(entry);
    }
    None
}
```

Each helper has one reason to change. Each is testable in isolation.

### 2.4 `keyword.rs` named keyword sets

```rust
/// Keyword inference — Phase 3 Step 3.1 — see Audit DOM-017.

const ERROR_KEYWORDS: &[&str] = &[
    "error", "exception", "fatal", "crash", "panic",
    "failed", "failure", "fail",
];

const WARNING_KEYWORDS: &[&str] = &[
    "warn", "warning", "deprecated", "caution",
];

const DEBUG_KEYWORDS: &[&str] = &[
    "debug", "trace", "verbose",
];

/// Build a word-boundary `(?i)` regex from a keyword list.
fn build_keyword_regex(keywords: &[&str]) -> Regex {
    let pattern = format!(r"(?i)\b({})\b", keywords.join("|"));
    Regex::new(&pattern).expect("keyword regex compiles")
}

static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(ERROR_KEYWORDS));
static WARNING_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(WARNING_KEYWORDS));
static DEBUG_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(DEBUG_KEYWORDS));
```

Three benefits: the keyword *set* is discoverable at a glance; tests can assert specific keywords are covered; `build_keyword_regex` is a pure helper that can be unit-tested without regex literal voodoo.

### 2.5 DOM-014 acknowledgement

At the top of each `static LazyLock<Regex>` cluster in `generic.rs`,
`structured.rs`, `keyword.rs`, add a one-line comment:
```rust
// LazyLock regex compilation is deliberate — compiles on first use, O(1)
// thereafter. Audit DOM-014 reviewed and approved. Do not replace with
// runtime-rebuilt regex without profiling first.
```

No behaviour change. Just discoverability.

---

## 3. 迁移策略

Order of operations within the step:

1. Introduce `parser/util.rs`, add `ANSI_RE` + `strip_ansi` there. (Task 1)
2. Replace the two duplicate `ANSI_RE` + call-site code in `generic.rs` and `structured.rs` with calls to `util::strip_ansi` / `util::ANSI_RE`. Delete the old `static ANSI_RE` entries. (Task 2)
3. Add `MultiStrategyParser::with_strategies`, make `default_chain` delegate. (Task 3)
4. Extract the three `generic.rs` flutter helpers. (Task 4)
5. Extract keyword sets in `keyword.rs`. (Task 5)
6. Add DOM-014 acknowledgement comments. (Task 6)
7. Verify Phase 3 exit gates and commit. (Task 7)

After each task: `cargo test` all green (every characterization test from Phase 2.5B must still pass), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`. Red → stop, fix or revert.

### What MUST stay unchanged

- **Behaviour**. Every parser characterization test written in Phase 2.5B
  Tasks 2+3 must still pass. These are the safety net; if one turns red,
  the redesign is wrong, not the test.
- **Public API**. Only additions: `parser::util::{ANSI_RE, strip_ansi}`,
  `MultiStrategyParser::with_strategies`. No renames. No deletions of
  existing public symbols.
- **Line count**. Each file stays under 500 (green), with `structured.rs`
  currently at 464 — keep it there. `generic.rs` 399 will grow slightly
  from helper additions but should not exceed 500.

### What MAY change

- Private helper function names inside `generic.rs` / `keyword.rs`.
- Number of `static LazyLock<Regex>` items (some may move to `util.rs`).
- Test imports in the in-file `#[cfg(test)] mod tests` (if a const or
  helper is newly pub(crate) and a test wants to assert on it).

### What MUST NOT change

- `ParseResult` / `LogEntry` / `LogLevel` / `InputSource` types.
- `LogLineParser` trait signature.
- Any behaviour captured in `src/parser/*.rs` #[cfg(test)] mod tests, or
  in `tests/characterization_*.rs` files touching parser.

---

## 4. File Structure

**New files:**
- `src/parser/util.rs` (~30 lines) — ANSI regex + `strip_ansi()` helper

**Modified files:**
- `src/parser/mod.rs` — add `pub mod util;`, add `with_strategies()` ctor, make `default_chain()` delegate, add DOM-014 comment
- `src/parser/generic.rs` — remove local `ANSI_RE`, use `util::ANSI_RE` / `strip_ansi`, extract 3 private helpers for flutter fallthrough, add DOM-014 comment
- `src/parser/structured.rs` — remove local `ANSI_RE`, use `util::strip_ansi`, add DOM-014 comment
- `src/parser/keyword.rs` — extract `ERROR_KEYWORDS`, `WARNING_KEYWORDS`, `DEBUG_KEYWORDS` constants + `build_keyword_regex` helper, add DOM-014 comment
- `src/parser/network.rs` — no changes expected (doesn't use ANSI or fallthrough)

Expected line counts after step:
- `util.rs` ~30
- `mod.rs` ~225 (+ ~10 lines for `with_strategies` + doc)
- `generic.rs` ~420 (+ ~20 lines for helpers, − ~10 lines from extracted `ANSI_RE`, net +10)
- `structured.rs` ~460 (− 3 lines from removed `static ANSI_RE`)
- `keyword.rs` ~105 (+ ~10 lines for consts + `build_keyword_regex` − 6 lines inline regex bodies)
- Total: ~1240 (was 1235) — essentially flat.

All files stay in **green zone (<500)**.

---

## 5. Per-step: execute after user approval

### Task 0: Pre-flight verification

**Files:** (read-only verification)

- [ ] **Step 0.1: Confirm HEAD**

Run: `git log --oneline -1`
Expected: `8713a72 docs(journal): Phase 2.5B — characterization tests complete` (or a later commit if one landed).

- [ ] **Step 0.2: Baseline tests + clippy + fmt**

Run:
```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check && echo fmt clean
```
Expected: all `ok`, clippy clean, fmt clean. Record the parser-specific test counts as a baseline:
```bash
cargo test --lib parser -- --list 2>&1 | grep -c ": test$"
cargo test --bin flog parser -- --list 2>&1 | grep -c ": test$"
```

- [ ] **Step 0.3: Record coverage baseline for this step**

Run:
```bash
cargo llvm-cov --summary-only 2>&1 | grep -E "parser/" > /tmp/phase3-step1-pre.txt
cat /tmp/phase3-step1-pre.txt
```
Save for comparison at Task 7.

---

### Task 1: Introduce `parser/util.rs`

**Files:**
- Create: `src/parser/util.rs`
- Modify: `src/parser/mod.rs` — add `pub mod util;`

- [ ] **Step 1.1: Create `src/parser/util.rs`**

Use `Write` with:
```rust
//! Shared regex helpers for all parser strategies.
//!
//! Phase 3 Step 3.1 extraction — see Audit DOM-015. Previously the
//! ANSI-strip regex was duplicated in `generic.rs` and `structured.rs`.
//! Centralising here means a single source of truth; updates (e.g.,
//! supporting OSC-8 hyperlinks) happen in one place.

use regex::Regex;
use std::sync::LazyLock;

/// ANSI escape sequence matcher (CSI-style, e.g. `\x1b[31m`).
///
/// LazyLock regex compilation is deliberate — compiles on first use,
/// O(1) thereafter. Audit DOM-014 reviewed and approved.
pub static ANSI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

/// Strip ANSI escape sequences from a string. Returns `Cow` so callers
/// don't allocate when no escapes are present.
pub fn strip_ansi(s: &str) -> std::borrow::Cow<'_, str> {
    ANSI_RE.replace_all(s, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_color() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
    }

    #[test]
    fn strip_ansi_handles_multiple_codes_in_one_line() {
        assert_eq!(
            strip_ansi("\x1b[1;31mbold red\x1b[0m plain \x1b[32mgreen"),
            "bold red plain green"
        );
    }

    #[test]
    fn strip_ansi_passes_through_when_no_codes() {
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn strip_ansi_handles_empty() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn strip_ansi_only_strips_csi_not_other_escapes() {
        // The regex is ESC[...m; OSC hyperlinks (ESC ] ...) are not
        // matched. Lock this behaviour — Phase 3+ extensions may add
        // OSC support.
        let osc = "\x1b]8;;https://example.com\x07link\x1b]8;;\x07";
        assert_eq!(strip_ansi(osc), osc);
    }

    #[test]
    fn ansi_re_is_shared_instance() {
        // Two is_match calls use the same compiled regex. This is
        // more documentation than test — it proves ANSI_RE is
        // reachable as a pub static.
        assert!(ANSI_RE.is_match("\x1b[0m"));
        assert!(!ANSI_RE.is_match("plain"));
    }
}
```

- [ ] **Step 1.2: Register the module**

Use `Edit` on `src/parser/mod.rs` to add `pub mod util;` after the existing `pub mod network;`:

Before:
```rust
pub mod generic;
pub mod keyword;
pub mod network;
pub mod structured;
```

After:
```rust
pub mod generic;
pub mod keyword;
pub mod network;
pub mod structured;
pub mod util;
```

- [ ] **Step 1.3: Run tests + clippy + fmt**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
```
Expected: 6 new tests pass in lib + bin (`parser::util::tests::*`). Clippy clean. Fmt clean. All existing tests still green.

- [ ] **Step 1.4: Commit Task 1**

```bash
git add src/parser/util.rs src/parser/mod.rs
git commit -m "$(cat <<'EOF'
refactor(parser): add parser/util.rs for shared ANSI helpers (Phase 3 DOM-015)

Introduces parser/util.rs with pub static ANSI_RE and pub fn strip_ansi.
Both will replace the duplicated copies in generic.rs and structured.rs
in the next task.

Adds 6 unit tests covering CSI removal, multi-code lines, passthrough,
empty, OSC-8-not-stripped (locks behaviour), and ANSI_RE reachability.

No change to public parser API beyond the new util module. Behaviour
unchanged — generic.rs and structured.rs still use their local copies
until Task 2.

Audit: docs/superpowers/audit/02-domain.md DOM-015
Step: docs/superpowers/plans/2026-04-23-phase3-step1-parser.md

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Dedupe ANSI regex — delete local copies, call `util::strip_ansi`

**Files:**
- Modify: `src/parser/generic.rs` — remove `static ANSI_RE`, change call sites
- Modify: `src/parser/structured.rs` — remove `static ANSI_RE`, change call sites

- [ ] **Step 2.1: Update `src/parser/generic.rs`**

Read `src/parser/generic.rs` first to find the exact line numbers (may have shifted during Phase 2.5B). Look for:
- `static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());`
- Every call site: `ANSI_RE.replace_all(...)`.

Edit 1 — remove the local static:
```rust
// Before (search for this):
static ANSI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

// After:
// (deleted; use super::util::ANSI_RE)
```

Edit 2 — update every `ANSI_RE.replace_all(raw, "")` call:
```rust
// Before:
let stripped = ANSI_RE.replace_all(line, "");

// After:
let stripped = super::util::strip_ansi(line);
```

If a call site returns `String`, change `.replace_all(...).to_string()` to `.strip_ansi(...).into_owned()`. If a call site uses `.to_string()` elsewhere, keep the same `.into_owned()` conversion.

If `ANSI_RE` is used more than three times in the file, prefer `super::util::ANSI_RE.replace_all(x, "")` so the diff stays small at call sites; but `strip_ansi` is preferred when the result is converted to an owned `String`.

After edits, remove the now-unused `use std::sync::LazyLock;` if no other `LazyLock` remains in the file. (Grep `LazyLock` in the file first to check.)

- [ ] **Step 2.2: Update `src/parser/structured.rs`**

Same treatment:
1. Delete `static ANSI_RE: LazyLock<Regex> = ...`.
2. Replace all `ANSI_RE.replace_all(...)` call sites with `super::util::strip_ansi(...)` or `super::util::ANSI_RE.replace_all(...)`.
3. `use std::sync::LazyLock;` stays because structured.rs has other `LazyLock` regex.

- [ ] **Step 2.3: Run tests + clippy + fmt**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
```
Expected: all test counts unchanged, clippy clean, fmt clean. If any parser characterization test goes red, the extraction broke behaviour — revert and investigate.

- [ ] **Step 2.4: Commit Task 2**

```bash
git add src/parser/generic.rs src/parser/structured.rs
git commit -m "$(cat <<'EOF'
refactor(parser): dedupe ANSI regex to parser::util (Phase 3 DOM-015)

Removes the two duplicated static ANSI_RE definitions in generic.rs and
structured.rs. Both now call super::util::strip_ansi / super::util::ANSI_RE
introduced in the previous commit.

Net: −6 lines per duplicate site, +1 use path per call site. Behaviour
unchanged (the regex pattern is identical to both previous copies, and
parser characterization tests all remain green).

Audit: docs/superpowers/audit/02-domain.md DOM-015

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `MultiStrategyParser::with_strategies`

**Files:**
- Modify: `src/parser/mod.rs`

- [ ] **Step 3.1: Add the constructor + refactor `default_chain`**

Read `src/parser/mod.rs:25-45` (the `impl MultiStrategyParser` block). Apply:

```rust
impl MultiStrategyParser {
    /// Create a parser chain with an explicit list of strategies.
    ///
    /// Callers decide the order. Useful for tests, custom parser
    /// injection, or A/B evaluation of alternative strategies.
    ///
    /// Phase 3 Step 3.1 — see Audit DOM-013.
    pub fn with_strategies(strategies: Vec<Box<dyn LogLineParser>>) -> Self {
        Self { strategies }
    }

    /// Create a parser chain with the default set of strategies, in
    /// priority order: Structured → Generic → Keyword.
    pub fn default_chain() -> Self {
        Self::with_strategies(vec![
            Box::new(structured::StructuredParser),
            Box::new(generic::GenericParser),
            Box::new(keyword::KeywordParser),
        ])
    }
}
```

The old `default_chain` body is discarded; everything flows through
`with_strategies`.

- [ ] **Step 3.2: Add a unit test asserting the new constructor works**

Read the existing `#[cfg(test)] mod tests` in `src/parser/mod.rs`. Append one test:

```rust
#[test]
fn with_strategies_preserves_order_and_delegates_to_first_match() {
    // Construct a chain with keyword-only (no structured or generic).
    // Feed a line that would parse as structured — verify it falls
    // through to keyword.
    let p = MultiStrategyParser::with_strategies(vec![Box::new(keyword::KeywordParser)]);
    let result = p.parse("[INFO][Tag] message");
    // KeywordParser accepts any non-empty line, infers level from keywords.
    match result {
        ParseResult::Entry(entry) => {
            assert_eq!(entry.tag, "App"); // keyword parser's default tag
        }
        other => panic!("expected Entry, got {:?}", other),
    }
}

#[test]
fn with_strategies_empty_chain_returns_no_match() {
    // Edge case: empty strategy list.
    let p = MultiStrategyParser::with_strategies(vec![]);
    let result = p.parse("any input");
    assert!(matches!(result, ParseResult::NoMatch));
}
```

(Replace `ParseResult::NoMatch` with the actual variant name if different; use Grep to confirm.)

- [ ] **Step 3.3: Run tests + clippy + fmt**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
```
Expected: +2 tests passing in parser::tests. Clippy clean. Fmt clean. Existing parser chain tests still pass.

- [ ] **Step 3.4: Commit Task 3**

```bash
git add src/parser/mod.rs
git commit -m "$(cat <<'EOF'
feat(parser): MultiStrategyParser::with_strategies for custom chains (Phase 3 DOM-013)

Adds `pub fn with_strategies(Vec<Box<dyn LogLineParser>>) -> Self`.
default_chain() now delegates to it — single source of truth for how
a parser chain is constructed.

Enables test injection, custom parser addition, and reorder experiments
without touching parser internals.

+2 unit tests: delegation to first matching strategy, empty-chain
no-match. Existing chain-order tests unchanged.

Audit: docs/superpowers/audit/02-domain.md DOM-013

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `generic.rs` flutter fallthrough extraction

**Files:**
- Modify: `src/parser/generic.rs`

- [ ] **Step 4.1: Read + locate the fallthrough block**

Read `src/parser/generic.rs`. Find the `fn try_parse` implementation and inside it locate:
```rust
if let Some(caps) = FLUTTER_PLAIN_RE.captures(line) {
    // ... nested if let ...
}
```

- [ ] **Step 4.2: Extract three private helpers**

Within `impl GenericParser` (add the helpers as associated functions; do NOT change `try_parse`'s behaviour):

```rust
impl GenericParser {
    /// Try to parse a `I/flutter (pid): ...` or `flutter: ...` style line.
    ///
    /// Returns `Some` when the flutter prefix matches. The embedded
    /// content may itself be `[LEVEL][Tag] msg` shaped — in that case,
    /// the level and tag are lifted out; otherwise the whole content
    /// is stored as a System-level message tagged `flutter`.
    ///
    /// Phase 3 Step 3.1 — see Audit DOM-016.
    fn try_parse_flutter_prefixed(line: &str) -> Option<LogEntry> {
        let caps = FLUTTER_PLAIN_RE.captures(line)?;
        let raw_content = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let content = super::util::strip_ansi(raw_content).into_owned();
        Some(
            Self::try_parse_flutter_structured(&content)
                .unwrap_or_else(|| Self::build_flutter_plain(content)),
        )
    }

    /// When flutter content starts with `[LEVEL][Tag]`, build a
    /// `LogEntry` with the lifted level and tag.
    fn try_parse_flutter_structured(content: &str) -> Option<LogEntry> {
        let caps = BRACKET_LEVEL_RE.captures(content)?;
        let level_str = caps.get(1)?.as_str();
        let tag = caps
            .get(2)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "flutter".to_string());
        let message = caps.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
        // LogLevel::from_str is the documented parse path.
        let level = crate::domain::LogLevel::from_str(level_str).unwrap_or(crate::domain::LogLevel::Info);
        Some(LogEntry {
            timestamp: String::new(),
            level,
            tag,
            message,
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: crate::domain::InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        })
    }

    /// Flutter content didn't match `[LEVEL][Tag]`; treat as a System
    /// message tagged `flutter` (including empty lines from `print('')`).
    fn build_flutter_plain(content: String) -> LogEntry {
        LogEntry {
            timestamp: String::new(),
            level: crate::domain::LogLevel::System,
            tag: "flutter".to_string(),
            message: content,
            extra_lines: Vec::new(),
            repeat_count: 1,
            source: crate::domain::InputSource::DirectSocket,
            error: None,
            stacktrace: None,
        }
    }
}
```

**IMPORTANT:** The exact shape of `LogEntry` built here must mirror the current inline version. Use `Read` on generic.rs first to confirm the field values (especially `source`, `error`, `stacktrace`, default values for `extra_lines` / `repeat_count` / `timestamp`). Do not substitute plausible values; copy verbatim.

- [ ] **Step 4.3: Replace the inline block in `try_parse`**

Replace:
```rust
if let Some(caps) = FLUTTER_PLAIN_RE.captures(line) {
    // ... 20-40 lines of inline fallthrough ...
    return Some(...);
}
```

With:
```rust
if let Some(entry) = Self::try_parse_flutter_prefixed(line) {
    return Some(entry);
}
```

- [ ] **Step 4.4: Add focused unit tests for each helper**

Inside the existing `#[cfg(test)] mod tests` in `generic.rs`, add:

```rust
#[test]
fn try_parse_flutter_prefixed_accepts_i_flutter_line() {
    let line = "I/flutter ( 1234): hello world";
    let entry = GenericParser::try_parse_flutter_prefixed(line).unwrap();
    assert_eq!(entry.tag, "flutter");
    assert_eq!(entry.message, "hello world");
}

#[test]
fn try_parse_flutter_prefixed_accepts_flutter_colon_line() {
    let line = "flutter: simple message";
    let entry = GenericParser::try_parse_flutter_prefixed(line).unwrap();
    assert_eq!(entry.tag, "flutter");
    assert_eq!(entry.message, "simple message");
}

#[test]
fn try_parse_flutter_prefixed_lifts_bracketed_level_and_tag() {
    let line = "flutter: [ERROR][Net] connection failed";
    let entry = GenericParser::try_parse_flutter_prefixed(line).unwrap();
    assert_eq!(entry.level, crate::domain::LogLevel::Error);
    assert_eq!(entry.tag, "Net");
    assert_eq!(entry.message, "connection failed");
}

#[test]
fn try_parse_flutter_prefixed_empty_content_yields_system() {
    let line = "flutter: ";
    let entry = GenericParser::try_parse_flutter_prefixed(line).unwrap();
    assert_eq!(entry.level, crate::domain::LogLevel::System);
    assert_eq!(entry.tag, "flutter");
    assert_eq!(entry.message, "");
}

#[test]
fn try_parse_flutter_prefixed_non_flutter_line_returns_none() {
    let line = "[INFO][Tag] not a flutter line";
    assert!(GenericParser::try_parse_flutter_prefixed(line).is_none());
}

#[test]
fn try_parse_flutter_structured_requires_bracket_shape() {
    assert!(GenericParser::try_parse_flutter_structured("no brackets here").is_none());
    let e = GenericParser::try_parse_flutter_structured("[INFO][Tag] msg").unwrap();
    assert_eq!(e.level, crate::domain::LogLevel::Info);
    assert_eq!(e.tag, "Tag");
}

#[test]
fn build_flutter_plain_always_system_level() {
    let e = GenericParser::build_flutter_plain("anything".to_string());
    assert_eq!(e.level, crate::domain::LogLevel::System);
    assert_eq!(e.tag, "flutter");
}
```

- [ ] **Step 4.5: Run tests + clippy + fmt**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
```
Expected: +7 new tests pass. Existing parser chain tests and characterization tests in tests/ remain green. Clippy clean. Fmt clean.

If any existing characterization test goes red (e.g., a test asserted the specific fallthrough behaviour and the extraction changed it), **revert and investigate** — the extraction must be behaviour-preserving.

- [ ] **Step 4.6: Commit Task 4**

```bash
git add src/parser/generic.rs
git commit -m "$(cat <<'EOF'
refactor(parser/generic): extract flutter fallthrough into helpers (Phase 3 DOM-016)

try_parse's 3-deep nested if-let for flutter prefix handling is replaced
by three private associated functions:

- try_parse_flutter_prefixed(line) — entry point, decides
  structured-vs-plain
- try_parse_flutter_structured(content) — lifts [LEVEL][Tag] out of
  flutter content
- build_flutter_plain(content) — infallible System-level fallback

Each helper has a single reason to change. Each is testable in isolation
(+7 unit tests covering happy paths + Bracket-lift + empty-content +
non-flutter rejection + infallibility of build_flutter_plain).

Behaviour unchanged — all prior parser characterization tests (locked
in Phase 2.5B) remain green.

Audit: docs/superpowers/audit/02-domain.md DOM-016

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: `keyword.rs` named keyword sets

**Files:**
- Modify: `src/parser/keyword.rs`

- [ ] **Step 5.1: Extract keyword constants and helper**

Read `src/parser/keyword.rs`. Apply:

Before:
```rust
static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(error|exception|fatal|crash|panic|fail(ed|ure)?)\b").unwrap()
});

static WARNING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(warn(ing)?|deprecated|caution)\b").unwrap());

static DEBUG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(debug|trace|verbose)\b").unwrap());
```

After:
```rust
/// Keyword sets used by the fallback parser to infer a log level from
/// free-form text. Extracted from inline regex — Phase 3 Step 3.1, see
/// Audit DOM-017. Single source of truth for "what counts as an ERROR
/// keyword", addable without regex-pattern surgery.
///
/// NOTE: the original regex used `fail(ed|ure)?` — captured "fail",
/// "failed", "failure". We spell them out here so future readers don't
/// need to mentally expand regex groups.
pub(crate) const ERROR_KEYWORDS: &[&str] = &[
    "error",
    "exception",
    "fatal",
    "crash",
    "panic",
    "fail",
    "failed",
    "failure",
];

/// Original regex used `warn(ing)?` — captured "warn", "warning".
pub(crate) const WARNING_KEYWORDS: &[&str] = &[
    "warn",
    "warning",
    "deprecated",
    "caution",
];

pub(crate) const DEBUG_KEYWORDS: &[&str] = &[
    "debug",
    "trace",
    "verbose",
];

/// Build a case-insensitive word-boundary regex from a keyword list.
///
/// Phase 3 Step 3.1 — see Audit DOM-017.
fn build_keyword_regex(keywords: &[&str]) -> Regex {
    let pattern = format!(r"(?i)\b({})\b", keywords.join("|"));
    Regex::new(&pattern).expect("keyword regex compiles")
}

static ERROR_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(ERROR_KEYWORDS));
static WARNING_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(WARNING_KEYWORDS));
static DEBUG_RE: LazyLock<Regex> = LazyLock::new(|| build_keyword_regex(DEBUG_KEYWORDS));
```

- [ ] **Step 5.2: Add unit tests for the keyword sets and builder**

Inside the existing `#[cfg(test)] mod tests` in `keyword.rs`, add:

```rust
#[test]
fn error_keywords_include_expected_set() {
    // Lock the current ERROR_KEYWORDS membership. Phase 3+ changes
    // must update this test deliberately.
    assert!(ERROR_KEYWORDS.contains(&"error"));
    assert!(ERROR_KEYWORDS.contains(&"exception"));
    assert!(ERROR_KEYWORDS.contains(&"fatal"));
    assert!(ERROR_KEYWORDS.contains(&"crash"));
    assert!(ERROR_KEYWORDS.contains(&"panic"));
    assert!(ERROR_KEYWORDS.contains(&"fail"));
    assert!(ERROR_KEYWORDS.contains(&"failed"));
    assert!(ERROR_KEYWORDS.contains(&"failure"));
}

#[test]
fn warning_keywords_include_expected_set() {
    assert!(WARNING_KEYWORDS.contains(&"warn"));
    assert!(WARNING_KEYWORDS.contains(&"warning"));
    assert!(WARNING_KEYWORDS.contains(&"deprecated"));
    assert!(WARNING_KEYWORDS.contains(&"caution"));
}

#[test]
fn debug_keywords_include_expected_set() {
    assert!(DEBUG_KEYWORDS.contains(&"debug"));
    assert!(DEBUG_KEYWORDS.contains(&"trace"));
    assert!(DEBUG_KEYWORDS.contains(&"verbose"));
}

#[test]
fn build_keyword_regex_is_case_insensitive() {
    let re = build_keyword_regex(&["foo", "bar"]);
    assert!(re.is_match("FOO"));
    assert!(re.is_match("bar"));
}

#[test]
fn build_keyword_regex_respects_word_boundary() {
    let re = build_keyword_regex(&["log"]);
    assert!(re.is_match("please log"));
    assert!(!re.is_match("prologue"));
}

#[test]
fn build_keyword_regex_handles_single_keyword() {
    let re = build_keyword_regex(&["alone"]);
    assert!(re.is_match("i am alone"));
}
```

- [ ] **Step 5.3: Run tests + clippy + fmt**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
```
Expected: +6 new tests pass. Existing `infers_error` / `infers_warning` / `infers_info_default` tests still pass because the regex produced from the const list is equivalent to the previous inline regex.

**Critical check**: the `infers_error` test feeds text like `"Something failed with an error"`. Both "failed" and "error" should match. Verify this still returns `LogLevel::Error` (not `Info`). If it fails, the const list is missing a keyword — compare to the original regex and adjust.

- [ ] **Step 5.4: Commit Task 5**

```bash
git add src/parser/keyword.rs
git commit -m "$(cat <<'EOF'
refactor(parser/keyword): extract keyword sets to named const (Phase 3 DOM-017)

ERROR_KEYWORDS, WARNING_KEYWORDS, DEBUG_KEYWORDS are now pub(crate) const
&[&str] lists. The three LazyLock<Regex> statics delegate to a new
build_keyword_regex(list) helper that produces the word-boundary case-
insensitive pattern from any list.

Enumerates the previously-regex-buried fail/failed/failure and
warn/warning variants explicitly in the list, so the keyword set is
discoverable at a glance instead of hidden inside `(?i)\b(fail(ed|ure)?)\b`.

+6 unit tests covering expected set membership (error/warning/debug),
case-insensitivity of the builder, word-boundary enforcement, and
single-keyword edge case. Existing infers_* tests unchanged.

Audit: docs/superpowers/audit/02-domain.md DOM-017

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: DOM-014 acknowledgement comments

**Files:**
- Modify: `src/parser/generic.rs`, `src/parser/structured.rs`, `src/parser/keyword.rs`

No behaviour change. Just add a one-line comment above each `static ... LazyLock<Regex>` cluster.

- [ ] **Step 6.1: Add comment in `src/parser/generic.rs`**

Read the file, find the block of `static ... LazyLock<Regex>` definitions near the top. Above the first one, add:

```rust
// LazyLock regex compilation is deliberate — compiles on first use, O(1)
// thereafter. Audit DOM-014 reviewed and approved. Do not replace with
// runtime-rebuilt regex without profiling first.
```

- [ ] **Step 6.2: Same for `src/parser/structured.rs`**

- [ ] **Step 6.3: Same for `src/parser/keyword.rs`** (above `ERROR_RE` or above the `ERROR_KEYWORDS` const, whichever groups the compile-once discussion best)

- [ ] **Step 6.4: Run verification**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check
```
Expected: no change in counts. Clippy + fmt clean.

- [ ] **Step 6.5: Commit Task 6**

```bash
git add src/parser/generic.rs src/parser/structured.rs src/parser/keyword.rs
git commit -m "$(cat <<'EOF'
docs(parser): acknowledge DOM-014 LazyLock regex pattern

Adds a one-line comment above each LazyLock<Regex> cluster in generic.rs,
structured.rs, keyword.rs pointing to Audit DOM-014's proposed_action
("No change needed. LazyLock is correct."). Future readers see the audit
trail without running grep.

No code change.

Audit: docs/superpowers/audit/02-domain.md DOM-014

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Step verification + step journal entry

- [ ] **Step 7.1: Full exit-gate check (per spec §5.4 (g))**

```bash
cargo test 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2
cargo fmt --check && echo fmt clean
```
All must be green.

- [ ] **Step 7.2: Characterization-test preservation audit**

Every Phase 2.5B parser characterization test must still pass. Run:
```bash
cargo test --lib parser 2>&1 | grep -E "test result|FAILED"
cargo test --bin flog parser 2>&1 | grep -E "test result|FAILED"
```
Expected: 0 failures. If any test that wasn't added in Tasks 1-6 went red, the redesign is wrong — investigate before proceeding.

- [ ] **Step 7.3: Coverage audit — should not regress**

```bash
cargo llvm-cov --summary-only 2>&1 | grep -E "parser/"
```
Compare to `/tmp/phase3-step1-pre.txt` from Task 0.3. Expected: every parser file stays ≥ its baseline (the redesign preserved behaviour; if coverage dropped, tests were accidentally deleted or a helper has no coverage — address before closing the step).

- [ ] **Step 7.4: File-size audit per spec §5.5**

```bash
wc -l src/parser/*.rs
```
Expected: every file < 500 (green zone). Current baseline total was 1235; post-Step ≈ 1240.

- [ ] **Step 7.5: Audit-entry checklist**

- [x] DOM-013 — `with_strategies` added, default_chain delegates — verify by grep
- [x] DOM-014 — acknowledgement comments added — verify by grep for "Audit DOM-014"
- [x] DOM-015 — `parser::util::{ANSI_RE, strip_ansi}` exists; no local ANSI_RE remains — verify by `grep -rn "static ANSI_RE" src/parser/ | wc -l` (expect 1, inside util.rs)
- [x] DOM-016 — three private helpers in generic.rs: `try_parse_flutter_prefixed`, `try_parse_flutter_structured`, `build_flutter_plain` — verify by grep
- [x] DOM-017 — `ERROR_KEYWORDS`, `WARNING_KEYWORDS`, `DEBUG_KEYWORDS` const + `build_keyword_regex` — verify by grep

Run:
```bash
echo "=== DOM-013 with_strategies ===" && grep -n "with_strategies" src/parser/mod.rs
echo "=== DOM-014 acknowledgements ===" && grep -rn "Audit DOM-014" src/parser/
echo "=== DOM-015 util module ===" && grep -rn "pub fn strip_ansi\|pub static ANSI_RE" src/parser/
echo "=== DOM-015 no local ANSI_RE ===" && grep -rn "^static ANSI_RE" src/parser/
echo "=== DOM-016 flutter helpers ===" && grep -n "fn try_parse_flutter_prefixed\|fn try_parse_flutter_structured\|fn build_flutter_plain" src/parser/generic.rs
echo "=== DOM-017 keyword const ===" && grep -n "ERROR_KEYWORDS\|WARNING_KEYWORDS\|DEBUG_KEYWORDS\|fn build_keyword_regex" src/parser/keyword.rs
```
Every entry must produce the expected match count.

- [ ] **Step 7.6: Write step journal**

Create `docs/superpowers/journal/phase3-step1.md` summarising what this step did (旧问题 → 新设计 → 迁移 → 验收). This feeds Phase 5's ARCHITECTURE.md + MODULES.md per spec §5.1 (item 6).

Use `Write` with a journal template derived from `phase-2.5b.md`'s structure:

```markdown
# Phase 3 Step 3.1 — Parser Layer Redesign (Journal)

## 入口
- 日期：2026-04-23
- Git HEAD at entry: 8713a72 (Phase 2.5B journal)
- Parser-related tests at entry: <count> (from Task 0.2)

## 实际变更
- New: src/parser/util.rs (ANSI_RE + strip_ansi)
- Modified: src/parser/mod.rs, generic.rs, structured.rs, keyword.rs
- No file exceeds 500 lines (see `wc -l src/parser/*.rs`)

## Audit entries resolved
- DOM-013 ✓ MultiStrategyParser::with_strategies added
- DOM-014 ✓ Acknowledged via comments (no code change per audit)
- DOM-015 ✓ ANSI regex deduped to parser::util
- DOM-016 ✓ Generic fallthrough split into 3 helpers
- DOM-017 ✓ Keyword sets extracted to named const + build_keyword_regex

## 新抽象职责一句话 (for MODULES.md, Phase 5)
- `parser::util`: shared regex helpers; single source of truth for ANSI stripping across every parser strategy.
- `MultiStrategyParser::with_strategies`: inject custom parser chains for testing or extension; `default_chain` delegates to it.
- `GenericParser::try_parse_flutter_*` helpers: each handles one branch of the flutter-prefix decision tree (entry / bracketed-level lift / plain fallback).
- `keyword::{ERROR,WARNING,DEBUG}_KEYWORDS` + `build_keyword_regex`: declarative keyword inference — the keyword set is a list, not a regex.

## 测试 delta
- +6 parser::util::tests (strip_ansi coverage)
- +2 parser::tests (with_strategies delegation + empty chain)
- +7 generic::tests (flutter helper coverage)
- +6 keyword::tests (keyword set + builder coverage)
- = +21 lib tests, +21 bin tests

## 出口 verdict (spec §5.4 (g))
- [ ] All A characterization tests still green
- [ ] No B tests in scope (parser has no B entries)
- [ ] All D characterization tests still green
- [ ] New structural tests added per helper
- [ ] wc -l src/parser/*.rs all < 500
- [ ] Every new module has a one-sentence responsibility (above)

## 意外发现
(Fill with anything that surprised the implementer; feeds Phase 6 retrospective.)

## 移交 Step 3.2 事项
- util module pattern is transferable — if Domain redesign needs a shared
  helper module, use this shape.
- MultiStrategyParser's `with_strategies` is an example of how Phase 3
  adds test seams without breaking existing public API.
```

- [ ] **Step 7.7: Commit the step journal**

```bash
git add docs/superpowers/journal/phase3-step1.md
git commit -m "$(cat <<'EOF'
docs(journal): Phase 3 Step 3.1 — parser redesign complete

Step journal documenting 旧问题 → 新设计 → 实际变更 → 验收 for the
parser-layer refactor.

5 DOM audit entries resolved (DOM-013/014/015/016/017). +21 lib / +21
bin tests added per spec §5.4 (e) "新测试断言新结构的合约". All prior
parser characterization tests remain green (behaviour unchanged).

Spec: docs/superpowers/specs/2026-04-22-project-cleanup-design.md §5
Plan: docs/superpowers/plans/2026-04-23-phase3-step1-parser.md

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
EOF
)"
```

---

## 6. Handoff to next step

Step 3.2 (Domain layer redesign) reads from this step's journal to see
the util-module pattern; it will likely introduce similar shared helpers
for the three filter enums (DOM-001) and for FlogNetMessage state
validation (DOM-002).

DO NOT start Step 3.2 planning until this step's Task 7 commit is on
master and the user has approved this step's design (Sections 1–3).

---

## 7. Self-review checklist

**Spec coverage** (per spec §5):
- [x] §5.1 (1) tests green after each task — Task verification steps enforce
- [x] §5.1 (2) A tests not red — Task 4 / 7 checks
- [x] §5.1 (3) B tests in scope — N/A (parser has no B entries)
- [x] §5.1 (4) new test per step — every Task adds unit tests
- [x] §5.1 (5) one module at a time — scope is `src/parser/` only
- [x] §5.1 (6) design-first — Sections 1–3 are the design
- [x] §5.1 (7) diff review — Task 7 step verification
- [x] §5.2 100% from A/B/D — 5 audit entries mapped to 6 tasks
- [x] §5.4 (a) read audit — Section 1
- [x] §5.4 (b) write design — this plan
- [x] §5.4 (c) rewrite code — Tasks 1-6
- [x] §5.4 (d) cargo test at each task — Task verification steps
- [x] §5.4 (e) new tests per task — enforced
- [x] §5.4 (f) diff review — Task 7 step verification
- [x] §5.4 (g) verdict checklist — Task 7.5
- [x] §5.4 (h) commits — one per task
- [x] §5.5 line count rule — Section 4 file sizes
- [x] §5.6 zero cycles — parser does not import from any other src/ module besides domain; no new cycles introduced
- [x] §5.6 public API minimized — Section 2 lists new pub items (2: `util::ANSI_RE`, `util::strip_ansi`; 1 new method `with_strategies`)
- [x] §5.8 red lines — no protocol or flog_dart change; no new deps; no CLI change

**Placeholder scan:** No "TBD", "implement later", "add validation" patterns.

**Type consistency:** `with_strategies`/`default_chain`/`strip_ansi`/`ANSI_RE`/`ERROR_KEYWORDS`/etc. names consistent across all task bodies.
