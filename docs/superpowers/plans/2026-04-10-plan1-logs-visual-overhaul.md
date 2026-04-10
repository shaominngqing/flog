# Plan 1: Logs View Visual Overhaul

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve the Logs view readability through row separators, error/warning backgrounds, level-based message colors, column reorder, tag colored pills, and selected row enhancement.

**Architecture:** Pure visual changes to `src/ui/mod.rs` and `src/ui/highlight.rs`. No new files, no data model changes, no new features. All changes are in the rendering layer.

**Tech Stack:** Rust, ratatui 0.29, Catppuccin Macchiato palette

---

### Task 1: Add Error/Warning Row Background Colors

**Files:**
- Modify: `src/ui/mod.rs` (lines 29-48 palette constants, lines 490-498 row_bg calculation)

- [ ] **Step 1: Add background color constants**

In `src/ui/mod.rs`, after the existing palette constants (around line 48), add:

```rust
const ERROR_ROW_BG: Color   = Color::Rgb(50, 30, 35);   // subtle dark red
const WARNING_ROW_BG: Color = Color::Rgb(50, 45, 30);   // subtle dark yellow
```

- [ ] **Step 2: Replace zebra-stripe logic with level-based backgrounds**

In `draw_log_list()`, find the `row_bg` calculation (around line 496):

```rust
let row_bg = if is_selected { SURFACE0 } else if vi % 2 == 1 { MANTLE } else { BASE };
```

Replace with:

```rust
let row_bg = if is_selected {
    SURFACE1
} else {
    match entry.level {
        LogLevel::Error => ERROR_ROW_BG,
        LogLevel::Warning => WARNING_ROW_BG,
        _ => BASE,
    }
};
```

- [ ] **Step 3: Build and verify visually**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): add error/warning row backgrounds, remove zebra striping"
```

---

### Task 2: Level-Based Message Text Colors

**Files:**
- Modify: `src/ui/mod.rs` (lines 55-64 `level_color` function, lines 496-498 `base` style)

- [ ] **Step 1: Update `level_color` function for message text**

The existing `level_color()` already maps levels to colors, but `Debug` and `Verbose` use similar muted colors. We need a new function for message text specifically. Add after `level_color()`:

```rust
/// Returns the foreground color for the message text, based on log level.
fn message_color(level: LogLevel) -> Color {
    match level {
        LogLevel::Error => RED,
        LogLevel::Warning => YELLOW,
        LogLevel::Info => TEXT,
        LogLevel::Debug => SUBTEXT0,
        LogLevel::Verbose => OVERLAY0,
        LogLevel::System => OVERLAY0,
    }
}
```

- [ ] **Step 2: Use `message_color` instead of `level_color` for the `base` style**

In `draw_log_list()`, find the line (around 496):

```rust
let base = Style::default().fg(lc).bg(row_bg);
```

Change to:

```rust
let mc = message_color(entry.level);
let base = Style::default().fg(mc).bg(row_bg);
```

Keep the existing `let lc = level_color(entry.level);` — it's still used by `level_pill` and other places.

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): level-based message text colors"
```

---

### Task 3: Row Underline Separators

**Files:**
- Modify: `src/ui/mod.rs` (draw_log_list entry rendering, around lines 540-625)

- [ ] **Step 1: Add underline to last line of each entry**

The rendering logic for each entry produces multiple lines (header + wrapped + extra). We need to add `Modifier::UNDERLINED` to the last visual line of each entry. The underline color comes from the foreground color of the span.

After the entry rendering block (after the `extra_lines` loop, just before the `if lines.len() >= height { break; }` at the end of the per-entry loop), insert logic to apply underline to the last line added for this entry. Find the section where lines are pushed and modify the approach:

We need a helper to apply underline to all spans in the last line. Add this helper function before `draw_log_list`:

```rust
/// Apply UNDERLINED modifier to all spans in a Line, using SURFACE0 as fg for the underline.
fn apply_row_underline(line: &mut Line<'static>, row_bg: Color) {
    for span in line.spans.iter_mut() {
        // Set underline via modifier; the underline color follows fg.
        // We create a thin separator by giving the padding spans a dim underline.
        span.style = span.style.add_modifier(Modifier::UNDERLINED);
    }
}
```

Then at the end of each entry's rendering (after extra_lines loop, before the `if lines.len() >= height { break; }`), apply underline to the last line that belongs to this entry:

```rust
// Apply underline separator to last line of this entry
if let Some(last_line) = lines.last_mut() {
    apply_row_underline(last_line, row_bg);
}
```

Note: For separator entries (tag == "────"), skip the underline since they already have visual separation.

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): add underline row separators between log entries"
```

---

### Task 4: Column Reorder — Level Before Timestamp

**Files:**
- Modify: `src/ui/mod.rs` (lines 540-548 header_spans construction, lines 588-606 empty_prefix)

- [ ] **Step 1: Reorder header_spans**

Find the `header_spans` construction (around line 540):

```rust
let header_spans: Vec<Span> = vec![
    cursor, bm,
    Span::styled(time, time_style),
    Span::styled(" ", dim_s),
    level_span,
    Span::styled(" ", dim_s),
    Span::styled(tag, tag_s),
    Span::styled(" ", dim_s),
];
```

Change to:

```rust
let header_spans: Vec<Span> = vec![
    cursor, bm,
    level_span,
    Span::styled(" ", dim_s),
    Span::styled(time, time_style),
    Span::styled(" ", dim_s),
    Span::styled(tag, tag_s),
    Span::styled(" ", dim_s),
];
```

- [ ] **Step 2: Update empty_prefix to match new column order**

Find the `empty_prefix` closure (around line 590):

```rust
let empty_prefix = |sel: bool, bg: Color| -> Vec<Span<'static>> {
    let cursor_s = if sel {
        Span::styled("▎", Style::default().fg(BLUE).bg(bg))
    } else {
        Span::styled(" ", Style::default().bg(bg))
    };
    let blank = Style::default().bg(bg);
    vec![
        cursor_s,
        Span::styled("  ", blank),                          // bookmark
        Span::styled(" ".repeat(TIME_WIDTH), blank),        // time
        Span::styled(" ", blank),                           // sep
        Span::styled(" ".repeat(LEVEL_WIDTH), blank),       // level
        Span::styled(" ", blank),                           // sep
        Span::styled(" ".repeat(TAG_WIDTH), blank),         // tag
        Span::styled(" ", blank),                           // sep
    ]
};
```

Change the order of time and level:

```rust
let empty_prefix = |sel: bool, bg: Color| -> Vec<Span<'static>> {
    let cursor_s = if sel {
        Span::styled("▎", Style::default().fg(BLUE).bg(bg))
    } else {
        Span::styled(" ", Style::default().bg(bg))
    };
    let blank = Style::default().bg(bg);
    vec![
        cursor_s,
        Span::styled("  ", blank),                          // bookmark
        Span::styled(" ".repeat(LEVEL_WIDTH), blank),       // level
        Span::styled(" ", blank),                           // sep
        Span::styled(" ".repeat(TIME_WIDTH), blank),        // time
        Span::styled(" ", blank),                           // sep
        Span::styled(" ".repeat(TAG_WIDTH), blank),         // tag
        Span::styled(" ", blank),                           // sep
    ]
};
```

- [ ] **Step 3: Update entry_row_count_from_store header_width comment**

Find `entry_row_count_from_store` (around line 706), update the comment to reflect new order:

```rust
// Header prefix width (must match render layout)
// cursor(1) + bookmark(2) + level(LEVEL_WIDTH) + sep(1) + time(TIME_WIDTH) + sep(1) + tag(TAG_WIDTH) + sep(1)
let header_width = 1 + 2 + LEVEL_WIDTH + 1 + TIME_WIDTH + 1 + TAG_WIDTH + 1;
```

The calculation stays the same (addition is commutative), only the comment changes.

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): reorder columns — level before timestamp"
```

---

### Task 5: Tag Colored Pills

**Files:**
- Modify: `src/ui/mod.rs` (lines 49 constants, lines 540-548 tag span construction)

- [ ] **Step 1: Add tag color assignment function**

Add after the existing palette constants:

```rust
const TAG_COLORS: [Color; 5] = [BLUE, GREEN, PEACH, MAUVE, SAPPHIRE];

/// Assign a consistent color to a tag name via simple hash.
fn tag_color(tag: &str) -> Color {
    let hash: usize = tag.bytes().fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    TAG_COLORS[hash % TAG_COLORS.len()]
}
```

- [ ] **Step 2: Replace tag text span with colored pill**

In `draw_log_list()`, find the tag span in `header_spans` (the line with `Span::styled(tag, tag_s)`):

```rust
Span::styled(tag, tag_s),
```

Replace with:

```rust
Span::styled(
    safe_pad(&entry.tag, TAG_WIDTH),
    Style::default().fg(MANTLE).bg(tag_color(&entry.tag)),
),
```

Remove the now-unused `tag_s` variable (`let tag_s = Style::default().fg(TEAL).bg(row_bg);`) and the `let tag = safe_pad(&entry.tag, TAG_WIDTH);` line (we inline it now).

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): tag colored pills with consistent hash-based colors"
```

---

### Task 6: Selected Row Enhancement

**Files:**
- Modify: `src/ui/mod.rs` (row_bg for selected, around line 496)

- [ ] **Step 1: Use SURFACE1 for selected row background**

This was already done in Task 1 when we replaced the `row_bg` calculation. Verify the selected row uses `SURFACE1`:

```rust
let row_bg = if is_selected {
    SURFACE1
} else {
    match entry.level {
        LogLevel::Error => ERROR_ROW_BG,
        LogLevel::Warning => WARNING_ROW_BG,
        _ => BASE,
    }
};
```

If it already says `SURFACE1`, this step is complete. If it says `SURFACE0`, change to `SURFACE1`.

- [ ] **Step 2: Commit (if changed)**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): use SURFACE1 for selected row background"
```

---

### Task 7: HTTP Highlight Enhancement

**Files:**
- Modify: `src/ui/highlight.rs` (highlight rules, lines 12-72)

- [ ] **Step 1: Update HTTP method rule to use pill style**

Find the HTTP method rule in `RULES` (around line 59):

```rust
// HTTP 方法
HighlightRule {
    regex: Regex::new(r"\b(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)\b").unwrap(),
    style: Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD),
},
```

Replace with pill style (MANTLE text on MAUVE background):

```rust
// HTTP 方法 — pill style
HighlightRule {
    regex: Regex::new(r"\b(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)\b").unwrap(),
    style: Style::default()
        .fg(Color::Rgb(30, 32, 48))     // MANTLE
        .bg(Color::Rgb(198, 160, 246))   // MAUVE
        .add_modifier(Modifier::BOLD),
},
```

- [ ] **Step 2: Add BOLD to HTTP status code rules**

Find the 2xx rule (around line 14):

```rust
HighlightRule {
    regex: Regex::new(r"\b[2]\d{2}\b").unwrap(),
    style: Style::default().fg(Color::Green),
},
```

Add BOLD:

```rust
HighlightRule {
    regex: Regex::new(r"\b[2]\d{2}\b").unwrap(),
    style: Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
},
```

Same for 4xx and 5xx rules — add `Modifier::BOLD` if not already present.

- [ ] **Step 3: Add UNDERLINED to slow duration rule**

Find the >1000ms rule (around line 30):

```rust
HighlightRule {
    regex: Regex::new(r"\((\d{4,})ms\)").unwrap(),
    style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
},
```

Add UNDERLINED:

```rust
HighlightRule {
    regex: Regex::new(r"\((\d{4,})ms\)").unwrap(),
    style: Style::default().fg(Color::Red).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
},
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add src/ui/highlight.rs
git commit -m "feat(ui): enhanced HTTP method pills, bold status codes, underlined slow requests"
```

---

### Task 8: Final Build and Test

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | head -30`
Expected: No errors (warnings acceptable for now).

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt -- --check 2>&1`
Expected: No formatting issues.

- [ ] **Step 4: Build release**

Run: `cargo build --release 2>&1 | head -10`
Expected: Compiles successfully.
