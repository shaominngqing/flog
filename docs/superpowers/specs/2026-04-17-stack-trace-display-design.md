# Stack Trace Display Optimization

## Problem

ERROR-level log entries with stack traces (especially Stack Overflow with 171K+ repeated frames) display poorly in flog:

1. **Log list**: ERROR entries only show the message first line; stack trace is invisible or fragmented
2. **Detail panel**: The `stacktrace` field on `LogEntry` is stored but never rendered — `full_message()` only joins `message` + `extra_lines`
3. **No frame folding**: Recursive stack traces (e.g., `_emit` repeated 171000 times) are shown verbatim, making them unreadable
4. **Weak visual distinction**: ERROR rows have subtle dark-red background but blend in with surrounding entries

## Design

### 1. Data Layer — `full_message()` Integrates `error` + `stacktrace`

**File**: `src/domain/entry.rs`

Update `full_message()` to append `error` and `stacktrace` fields with section separators:

```
{message}
{extra_lines...}
── Error ──
{error}
── Stack Trace ──
{stacktrace}
```

Only include `── Error ──` / `── Stack Trace ──` sections when the respective fields are `Some`.

### 2. Stack Frame Collapsing — `collapse_stack_frames()`

**File**: `src/domain/entry.rs` (new function)

Pure function that takes a stack trace string and returns collapsed lines.

**Algorithm**:
1. Split by `\n`
2. Detect Dart stack frames via `#\d+\s+` prefix
3. Group consecutive frames where the function name + file location are identical (ignore frame number)
4. For groups of size > 1: emit one line like `{function} ({file}:{line}) x {count}`
5. Non-frame lines (e.g., `Error: Stack Overflow`) pass through unchanged

**Output**: `Vec<String>` of collapsed lines.

### 3. Log List — Stack Trace Summary

**File**: `src/ui/logs/mod.rs`

For entries that have `error` or `stacktrace` fields:

```
▎ 06:37:46  ERROR  AzureLive  Parse error: Stack Overflow          ← line 1: message
                                Error: Stack Overflow               ← line 2: error summary (RED dimmed)
                                #0  _emit (azure_live_...:25)       ← line 3: first frame
                                _emit (azure_live_...:27) x 171000  ← line 4: collapsed repeated
                                ... 5 more frames                   ← line 5: truncation hint
```

**Constants**:
- `MAX_STACK_PREVIEW_LINES = 5` — max collapsed stack trace lines in list view
- After collapsing, if still > MAX, show first MAX lines + `... N more frames`

**Colors**:
- Error summary line: RED (`#ed8796`)
- Stack trace frames: OVERLAY0 (`#6e738d`) — dimmer than message
- Truncation hint (`... N more`): OVERLAY0 italic

### 4. Detail Panel — Full Display with Sections

**File**: `src/ui/logs/detail.rs`

The detail panel already renders `full_message()` through `json_viewer::bracket_format()`, which splits on `\n`. After updating `full_message()`, the detail panel will automatically show the full error + stack trace.

Enhancements:
- Apply `collapse_stack_frames()` to the stacktrace portion before displaying
- Section separators (`── Error ──`, `── Stack Trace ──`) rendered in SURFACE0 color
- Stack trace frame lines use OVERLAY0 color (dimmer than message RED)
- Collapsed frame count (`x N`) highlighted in PINK

### 5. `entry_row_count_from_store()` Update

**File**: `src/ui/logs/mod.rs`

Update the row count calculation to account for the new error/stacktrace preview lines, so scroll calculations remain correct.

### 6. Visual Enhancement

- ERROR entries: left cursor bar (`▎`) uses RED instead of BLUE when selected

## Files Changed

| File | Change |
|------|--------|
| `src/domain/entry.rs` | Update `full_message()`, add `collapse_stack_frames()`, add `collapsed_stack_preview()` |
| `src/ui/logs/mod.rs` | Render error summary + collapsed stack preview in list view, update row count |
| `src/ui/logs/detail.rs` | Apply collapsed stack trace rendering, section separator colors |

## Non-Goals

- Changing parser behavior for logcat/stdin mode (out of scope, user uses Direct Socket)
- Adding interactive expand/collapse for stack traces in list view (future enhancement)
- Modifying flog_dart protocol (stackTrace field already exists)
