# Phase 3 Step 3.9 — UI Shared Components

> REQUIRED SUB-SKILL: superpowers:subagent-driven-development or executing-plans.

**Goal:** Split the remaining >500-line UI shared modules. Resolve UI-012 (rename `source_select` → `device_picker`), UI-015 (input_field palette decoupling).

**Files exceeding 500 lines:**
- `src/ui/source_select.rs` 889
- `src/ui/json_viewer/render.rs` 745
- `src/ui/text_editor.rs` 620
- `src/ui/json_viewer/colorize.rs` 538

**Red line:** no render output changes. All characterization tests green at every commit. UI-012 rename is textual only — no behavior change.

## Tasks

### Task 0 — pre-flight
Verify HEAD, `cargo test --all` green, clippy clean, fmt clean.

### Task 1 — UI-012: rename `source_select` → `device_picker`
`git mv src/ui/source_select.rs src/ui/device_picker.rs`. Update `src/ui/mod.rs` module declaration. Find + replace `use crate::ui::source_select` → `use crate::ui::device_picker` across the codebase (grep for `source_select`). Update module-level doc comment to reflect accurate purpose. No functional change. `cargo test --all` green.
Commit: `refactor(ui): rename source_select → device_picker (Phase 3 UI-012)`

### Task 2 — Split `ui/device_picker.rs` (889 lines)
`git mv src/ui/device_picker.rs src/ui/device_picker/mod.rs` (directory conversion). Extract:
- `device_picker/modal.rs` — the draw_device_picker modal overlay render
- `device_picker/row.rs` — per-device row rendering (device header, app sub-items)
- `device_picker/click_map.rs` — click-region computation (currently embedded in draw fn)

Target: every `device_picker/*.rs` < 400 lines. Keep `pub fn draw_device_picker` as the public entry point in `mod.rs`.
Commit: `refactor(ui/device_picker): split modal/row/click_map (Phase 3 UI-038 mirror)`

### Task 3 — Split `ui/json_viewer/render.rs` (745 lines)
Extract:
- `json_viewer/render/lines.rs` — flat-to-Line conversion (the render loop body)
- `json_viewer/render/summaries.rs` — DevTools-style collapsed-summary rendering (`{k: v, …}`, `[v, …] (N)`, CJK truncation)
- `json_viewer/render.rs` → becomes `json_viewer/render/mod.rs` — public entry + depth-cycle helpers

Target: every `json_viewer/render/*.rs` < 400 lines.
Commit: `refactor(ui/json_viewer): split render into lines + summaries (Phase 3 UI-030 mirror)`

### Task 4 — Split `ui/text_editor.rs` (620 lines)
Extract:
- `text_editor/cursor.rs` — cursor/edit ops (insert_char, delete_char, backspace, newline, move_up/down/left/right)
- `text_editor/viewport.rs` — viewport/scroll logic (ensure_cursor_visible, render viewport slice)
- `text_editor.rs` → `text_editor/mod.rs` — struct + public entry + render coordination

Target: every `text_editor/*.rs` < 400 lines. Public API (`TextEditor` struct) preserved.
Commit: `refactor(ui/text_editor): split cursor + viewport (Phase 3 UI-014 mirror)`

### Task 5 — Split `ui/json_viewer/colorize.rs` (538 lines)
Extract:
- `colorize/lexer.rs` — token scanning (string, number, bool, null, punctuation)
- `colorize.rs` → `colorize/mod.rs` — token-to-span coloring + public entry

Target: each file < 400 lines.
Commit: `refactor(ui/json_viewer): split colorize lexer (Phase 3 UI-031 mirror)`

### Task 6 — UI-015: input_field palette decoupling
Expand `InputFieldProps` (currently reads palette constants implicitly from parent module scope) with explicit `bg: Color`, `fg: Color`, `cursor_style: Style` fields. Provide a default `InputFieldProps::with_default_style(...)` factory so existing call sites only add one line. Update callers in `ui/logs/toolbar.rs` and `ui/network/filter.rs`.
+1 test asserting custom palette propagates to rendered cells.
Commit: `refactor(ui/input_field): explicit style props (Phase 3 UI-015)`

### Task 7 — exit gate + journal
Full `cargo test --all` + clippy -D + fmt. Verify every `src/ui/**/*.rs` < 500 lines. Write `docs/superpowers/journal/phase3-step9.md` with file deltas + UI-012 rename summary + +1 test (Task 6).
Commit: `docs(journal): Phase 3 Step 3.9 — UI shared components complete`

## Exit gates
- All characterization tests green (same counts as entry)
- Every `src/ui/**/*.rs` < 500 lines
- UI-012 rename complete (no remaining references to `source_select` outside historical docs/journals)
- clippy -D warnings clean

## 红线
- No render output changes — every characterization assertion holds.
- Public API signatures preserved (`ui::device_picker::draw_device_picker`, `ui::json_viewer::*`, `ui::text_editor::TextEditor`, `ui::input_field::render_input_field`).
- UI-012 rename is internal — no effect on session.rs serialized field names.
- No new deps.
