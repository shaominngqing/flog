# Stack Trace Display Optimization — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ERROR-level log entries with stack traces readable — fold repeated frames, show previews in the list, display complete content with sections in detail panel.

**Architecture:** Add a `collapse_stack_frames()` pure function in `domain/entry.rs` that detects and folds consecutive identical stack frames. Update `full_message()` to include `error` and `stacktrace` fields. Modify list renderer to show a capped stack preview for error entries, and detail renderer to display sections with dimmer stack trace styling.

**Tech Stack:** Rust, ratatui, regex (already in deps)

---

### Task 1: Add `collapse_stack_frames()` to domain layer

**Files:**
- Modify: `src/domain/entry.rs`

- [ ] **Step 1: Write the failing test**

Add at the bottom of `src/domain/entry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_repeated_frames() {
        let input = "\
#0      Foo._emit (package:app/foo.dart:25:3)
#1      Foo._emit (package:app/foo.dart:27:5)
#2      Foo._emit (package:app/foo.dart:27:5)
#3      Foo._emit (package:app/foo.dart:27:5)
#4      Bar.run (package:app/bar.dart:10:7)";

        let result = collapse_stack_frames(input);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "#0      Foo._emit (package:app/foo.dart:25:3)");
        assert!(result[1].contains("× 3"));
        assert!(result[1].contains("Foo._emit"));
        assert_eq!(result[2], "#4      Bar.run (package:app/bar.dart:10:7)");
    }

    #[test]
    fn collapse_no_repeats() {
        let input = "\
#0      Foo.a (package:app/foo.dart:1:1)
#1      Bar.b (package:app/bar.dart:2:2)
#2      Baz.c (package:app/baz.dart:3:3)";

        let result = collapse_stack_frames(input);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "#0      Foo.a (package:app/foo.dart:1:1)");
    }

    #[test]
    fn collapse_preserves_non_frame_lines() {
        let input = "\
Error: Stack Overflow
#0      Foo._emit (package:app/foo.dart:25:3)
#1      Foo._emit (package:app/foo.dart:27:5)
#2      Foo._emit (package:app/foo.dart:27:5)";

        let result = collapse_stack_frames(input);
        assert_eq!(result[0], "Error: Stack Overflow");
        assert_eq!(result[1], "#0      Foo._emit (package:app/foo.dart:25:3)");
        assert!(result[2].contains("× 2"));
    }

    #[test]
    fn collapse_empty_input() {
        let result = collapse_stack_frames("");
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn collapse_single_frame() {
        let input = "#0      Foo.bar (package:app/foo.dart:1:1)";
        let result = collapse_stack_frames(input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], input);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib domain::entry::tests -- --nocapture`
Expected: FAIL — `collapse_stack_frames` not found.

- [ ] **Step 3: Implement `collapse_stack_frames()`**

Add above the `#[cfg(test)]` block in `src/domain/entry.rs`:

```rust
/// Extract the function+location signature from a Dart stack frame line.
/// Input like `#0      Foo._emit (package:app/foo.dart:25:3)` → `Foo._emit (package:app/foo.dart:25:3)`
/// Returns None for non-frame lines.
fn frame_signature(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    // Skip `#NNN` and whitespace
    let after_hash = &trimmed[1..];
    let after_num = after_hash.trim_start_matches(|c: char| c.is_ascii_digit());
    if after_num.is_empty() || !after_num.starts_with(char::is_whitespace) {
        return None;
    }
    Some(after_num.trim_start())
}

/// Collapse consecutive identical stack frames into `{signature} × N` lines.
/// Non-frame lines pass through unchanged.
pub fn collapse_stack_frames(stacktrace: &str) -> Vec<String> {
    let lines: Vec<&str> = stacktrace.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let sig = frame_signature(lines[i]);
        if let Some(current_sig) = sig {
            // Count consecutive frames with the same signature
            let start = i;
            i += 1;
            while i < lines.len() {
                if let Some(next_sig) = frame_signature(lines[i]) {
                    if next_sig == current_sig {
                        i += 1;
                        continue;
                    }
                }
                break;
            }
            let count = i - start;
            if count == 1 {
                result.push(lines[start].to_string());
            } else {
                // Keep the first occurrence as-is
                result.push(lines[start].to_string());
                // Add collapsed line for repeats
                result.push(format!("        {} × {}", current_sig, count - 1));
            }
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    result
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib domain::entry::tests -- --nocapture`
Expected: All 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/entry.rs
git commit -m "feat: add collapse_stack_frames() for folding repeated stack frames"
```

---

### Task 2: Update `full_message()` to include error + stacktrace

**Files:**
- Modify: `src/domain/entry.rs`

- [ ] **Step 1: Write the failing test**

Add to the existing `tests` module in `src/domain/entry.rs`:

```rust
    #[test]
    fn full_message_includes_error_and_stacktrace() {
        let mut entry = LogEntry::new(LogLevel::Error, "Test", "Parse error");
        entry.error = Some("Stack Overflow".to_string());
        entry.stacktrace = Some(
            "#0      Foo._emit (package:app/foo.dart:25:3)\n\
             #1      Foo._emit (package:app/foo.dart:27:5)\n\
             #2      Foo._emit (package:app/foo.dart:27:5)"
                .to_string(),
        );

        let msg = entry.full_message();
        assert!(msg.contains("Parse error"));
        assert!(msg.contains("── Error ──"));
        assert!(msg.contains("Stack Overflow"));
        assert!(msg.contains("── Stack Trace ──"));
        assert!(msg.contains("× 2")); // collapsed frames
    }

    #[test]
    fn full_message_no_error_no_stacktrace() {
        let entry = LogEntry::new(LogLevel::Info, "Test", "Hello world");
        let msg = entry.full_message();
        assert_eq!(msg, "Hello world");
        assert!(!msg.contains("── Error ──"));
    }

    #[test]
    fn full_message_error_only_no_stacktrace() {
        let mut entry = LogEntry::new(LogLevel::Error, "Test", "Crash");
        entry.error = Some("NullPointerException".to_string());

        let msg = entry.full_message();
        assert!(msg.contains("── Error ──"));
        assert!(msg.contains("NullPointerException"));
        assert!(!msg.contains("── Stack Trace ──"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib domain::entry::tests -- --nocapture`
Expected: 3 new tests FAIL — `full_message()` doesn't include error/stacktrace.

- [ ] **Step 3: Update `full_message()`**

Replace the `full_message()` method in `src/domain/entry.rs`:

```rust
    /// Complete message including continuation lines, error, and collapsed stacktrace.
    pub fn full_message(&self) -> String {
        let mut s = self.message.clone();
        for line in &self.extra_lines {
            s.push('\n');
            s.push_str(line);
        }
        if let Some(ref err) = self.error {
            s.push_str("\n\n── Error ──\n");
            s.push_str(err);
        }
        if let Some(ref st) = self.stacktrace {
            s.push_str("\n\n── Stack Trace ──\n");
            let collapsed = collapse_stack_frames(st);
            for line in &collapsed {
                s.push('\n');
                s.push_str(line);
            }
        }
        s
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib domain::entry::tests -- --nocapture`
Expected: All 8 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/entry.rs
git commit -m "feat: full_message() now includes error and collapsed stacktrace sections"
```

---

### Task 3: Add `stack_preview_lines()` for log list rendering

**Files:**
- Modify: `src/domain/entry.rs`

This method returns a capped preview of the error + stacktrace for list view rendering.

- [ ] **Step 1: Write the failing test**

Add to the existing `tests` module in `src/domain/entry.rs`:

```rust
    #[test]
    fn stack_preview_basic() {
        let mut entry = LogEntry::new(LogLevel::Error, "Test", "Parse error");
        entry.error = Some("Stack Overflow".to_string());
        entry.stacktrace = Some(
            "#0      Foo._emit (package:app/foo.dart:25:3)\n\
             #1      Foo._emit (package:app/foo.dart:27:5)\n\
             #2      Foo._emit (package:app/foo.dart:27:5)\n\
             #3      Bar.run (package:app/bar.dart:10:7)\n\
             #4      Baz.start (package:app/baz.dart:20:3)\n\
             #5      Main.go (package:app/main.dart:5:1)\n\
             #6      Root.init (package:app/root.dart:1:1)\n\
             #7      App.launch (package:app/app.dart:99:2)"
                .to_string(),
        );

        let (lines, remaining) = entry.stack_preview_lines(5);
        assert_eq!(lines[0], "Error: Stack Overflow");
        assert!(lines[1].contains("Foo._emit"));
        assert!(lines.len() <= 5);
        assert!(remaining > 0);
    }

    #[test]
    fn stack_preview_no_error_no_stack() {
        let entry = LogEntry::new(LogLevel::Info, "Test", "Hello");
        let (lines, remaining) = entry.stack_preview_lines(5);
        assert!(lines.is_empty());
        assert_eq!(remaining, 0);
    }

    #[test]
    fn stack_preview_short_stack() {
        let mut entry = LogEntry::new(LogLevel::Error, "Test", "Oops");
        entry.stacktrace = Some(
            "#0      Foo.bar (package:app/foo.dart:1:1)\n\
             #1      Baz.qux (package:app/baz.dart:2:2)"
                .to_string(),
        );

        let (lines, remaining) = entry.stack_preview_lines(5);
        assert_eq!(lines.len(), 2); // no error, just 2 frames
        assert_eq!(remaining, 0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib domain::entry::tests -- --nocapture`
Expected: 3 new tests FAIL.

- [ ] **Step 3: Implement `stack_preview_lines()`**

Add to the `impl LogEntry` block in `src/domain/entry.rs`:

```rust
    /// Returns a capped preview of error + collapsed stacktrace for list view.
    /// Returns (lines, remaining_count) where remaining_count is how many more lines exist beyond the cap.
    pub fn stack_preview_lines(&self, max_lines: usize) -> (Vec<String>, usize) {
        let mut lines: Vec<String> = Vec::new();

        if let Some(ref err) = self.error {
            lines.push(format!("Error: {}", err));
        }

        if let Some(ref st) = self.stacktrace {
            let collapsed = collapse_stack_frames(st);
            lines.extend(collapsed);
        }

        if lines.len() <= max_lines {
            (lines, 0)
        } else {
            let remaining = lines.len() - max_lines;
            lines.truncate(max_lines);
            (lines, remaining)
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib domain::entry::tests -- --nocapture`
Expected: All 11 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/entry.rs
git commit -m "feat: add stack_preview_lines() for capped stack trace previews"
```

---

### Task 4: Render stack trace preview in log list view

**Files:**
- Modify: `src/ui/logs/mod.rs`

- [ ] **Step 1: Add constant and render stack preview lines after extra_lines**

In `src/ui/logs/mod.rs`, add the constant near the other constants (around line 44):

```rust
/// Max collapsed stack trace preview lines shown in the log list.
const MAX_STACK_PREVIEW_LINES: usize = 5;
```

Then in the main render loop, after the `// Extra lines (continuation / stacktrace)` block (after line 852), add the stack trace preview rendering. Find this code:

```rust
            // Extra lines (continuation / stacktrace)
            let cont = Style::default().fg(lc).bg(row_bg);
            for extra in &entry.extra_lines {
                // ...existing code...
            }
```

After the closing brace of the `for extra in &entry.extra_lines` loop, add:

```rust
            // Stack trace preview (error + collapsed stacktrace)
            if entry.error.is_some() || entry.stacktrace.is_some() {
                let (preview, remaining) = entry.stack_preview_lines(MAX_STACK_PREVIEW_LINES);
                let err_style = Style::default().fg(RED).bg(row_bg).add_modifier(Modifier::DIM);
                let frame_style = Style::default().fg(OVERLAY0).bg(row_bg);

                for (pi, pline) in preview.iter().enumerate() {
                    if lines.len() >= height {
                        break;
                    }
                    let mut ps = empty_prefix(is_selected, row_bg);
                    let style = if pi == 0 && entry.error.is_some() {
                        err_style // First line is the error summary → RED dimmed
                    } else {
                        frame_style // Stack frames → OVERLAY0
                    };
                    ps.push(Span::styled(
                        safe_pad(pline, wrap_width),
                        style,
                    ));
                    let used: usize = ps.iter().map(|s| s.content.width()).sum();
                    if used < total_width {
                        ps.push(Span::styled(
                            " ".repeat(total_width - used),
                            Style::default().bg(row_bg),
                        ));
                    }
                    lines.push(Line::from(ps));
                    row_map.push(fi);
                }

                if remaining > 0 && lines.len() < height {
                    let mut ts = empty_prefix(is_selected, row_bg);
                    ts.push(Span::styled(
                        format!("... {} more frames", remaining),
                        Style::default().fg(OVERLAY0).bg(row_bg).add_modifier(Modifier::ITALIC),
                    ));
                    let used: usize = ts.iter().map(|s| s.content.width()).sum();
                    if used < total_width {
                        ts.push(Span::styled(
                            " ".repeat(total_width - used),
                            Style::default().bg(row_bg),
                        ));
                    }
                    lines.push(Line::from(ts));
                    row_map.push(fi);
                }
            }
```

- [ ] **Step 2: Update `entry_row_count_from_store()` to account for stack preview**

In the `entry_row_count_from_store()` function (around line 911), after the `extra_rows` calculation, add:

```rust
    let mut stack_rows = 0;
    if entry.error.is_some() || entry.stacktrace.is_some() {
        let (preview, remaining) = entry.stack_preview_lines(MAX_STACK_PREVIEW_LINES);
        stack_rows = preview.len();
        if remaining > 0 {
            stack_rows += 1; // "... N more frames" line
        }
    }
    wrapped.len() + extra_rows + stack_rows
```

- [ ] **Step 3: Build and verify**

Run: `cargo build`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src/ui/logs/mod.rs
git commit -m "feat: render stack trace preview in log list with frame collapsing"
```

---

### Task 5: Enhance detail panel stack trace display

**Files:**
- Modify: `src/ui/logs/detail.rs`

The detail panel already renders `full_message()` through `json_viewer::bracket_format()` which splits on `\n`. After Task 2, it automatically shows error + collapsed stacktrace. This task adds visual enhancements.

- [ ] **Step 1: Add section separator styling in detail panel**

In `src/ui/logs/detail.rs`, the body currently uses `json_viewer::bracket_format(&full_msg)` which sends everything to the JSON viewer. For non-JSON content (like stack traces), the viewer just splits on `\n` and shows plain text. The section headers (`── Error ──`, `── Stack Trace ──`) and collapsed counts (`× N`) will already display as plain lines.

No code changes needed for basic display — `full_message()` update from Task 2 handles this.

The json_viewer already handles newlines correctly via `bracket_format()` → the `else` branch at line 118 of `json_viewer.rs` splits by `text.lines()`.

- [ ] **Step 2: Verify detail panel renders correctly**

Run: `cargo build`
Expected: Compiles. Detail panel will now show error + stacktrace sections for entries that have them.

- [ ] **Step 3: Commit**

```bash
git add src/ui/logs/detail.rs
git commit -m "docs: confirm detail panel inherits stack trace display from full_message()"
```

---

### Task 6: Visual enhancement — RED cursor for ERROR entries

**Files:**
- Modify: `src/ui/logs/mod.rs`

- [ ] **Step 1: Update cursor color for ERROR entries**

In the log list render loop in `src/ui/logs/mod.rs`, find the cursor rendering (around line 659):

```rust
            let cursor = if is_selected {
                Span::styled("▎", Style::default().fg(BLUE).bg(row_bg))
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            };
```

Replace with:

```rust
            let cursor_color = if entry.level == LogLevel::Error { RED } else { BLUE };
            let cursor = if is_selected {
                Span::styled("▎", Style::default().fg(cursor_color).bg(row_bg))
            } else {
                Span::styled(" ", Style::default().bg(row_bg))
            };
```

- [ ] **Step 2: Build and verify**

Run: `cargo build`
Expected: Compiles. ERROR entries now show red cursor bar when selected.

- [ ] **Step 3: Commit**

```bash
git add src/ui/logs/mod.rs
git commit -m "feat: red cursor bar for ERROR-level log entries"
```

---

### Task 7: Full build + test verification

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No warnings.

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: Final commit if any formatting fixes needed**

```bash
cargo fmt
git add -A
git commit -m "style: format code"
```
