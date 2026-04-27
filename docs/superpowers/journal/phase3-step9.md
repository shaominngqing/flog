# Phase 3 Step 3.9 — UI Shared Components (journal)

**Scope:** Split the remaining >500-line UI files; rename `source_select` → `device_picker` (UI-012); decouple `input_field` palette (UI-015).

## Commits

| SHA | Task | Summary |
|---|---|---|
| `0441135` | 1 | refactor(ui): rename source_select → device_picker (UI-012) |
| `b4da476` | 2 | refactor(ui/device_picker): split modal/row/card/click_map (UI-038 mirror) |
| `f1e71a8` | 3 | refactor(ui/json_viewer): split render into lines + summaries (UI-030 mirror) |
| `ab0cfd7` | 4 | refactor(ui/text_editor): split cursor + viewport (UI-014 mirror) |
| `9d7d947` | 5 | refactor(ui/json_viewer): split colorize lexer (UI-031 mirror) |
| `eee71ad` | 6 | refactor(ui/input_field): explicit style props (UI-015) |
| `09c07a7` | 6b | refactor(ui/input_field): move tests into dedicated tests.rs file |

## File deltas (all <500 lines)

Before Step 3.9:
- `source_select.rs` 889
- `json_viewer/render.rs` 745
- `text_editor.rs` 620
- `json_viewer/colorize.rs` 538
- `input_field.rs` 481

After Step 3.9:
- `device_picker/mod.rs` + `modal.rs` + `row.rs` + `card.rs` + `click_map.rs` — each <400
- `json_viewer/render/{mod.rs, lines.rs, summaries.rs}` — largest 339
- `text_editor/{mod.rs, cursor.rs, viewport.rs}` — largest 422
- `json_viewer/colorize/{mod.rs, lexer.rs}` — largest 476
- `input_field/{mod.rs, tests.rs}` — 267 + 263

Current `src/ui/**/*.rs` largest 10:
```
493 ui/network/mock_rules.rs       (out of scope — under 500)
476 ui/json_viewer/colorize/mod.rs
474 ui/logs/list.rs
471 ui/network/stats.rs
439 ui/mod.rs
422 ui/text_editor/mod.rs
409 ui/logs/detail/renderers.rs
352 ui/device_picker/card.rs
351 ui/tab_bar.rs
339 ui/json_viewer/render/mod.rs
```

## Deviations from plan

1. **Task 6 follow-up commit** (`09c07a7`): the `InputFieldProps` palette decoupling added a `with_default_style` factory + custom-palette tests that pushed `input_field/mod.rs` to 530 lines. Rather than amend, committed the test-file split as `Task 6b` (same pattern used in Step 3.7 for `list.rs`/`highlight.rs`).
2. **Subagent turn truncated** during Task 7 preparation (API event-size limit). All code commits for Tasks 1-6 already landed; controller resumed, ran the fit-the-budget commit (Task 6b), final test/clippy/fmt gates, and wrote this journal directly.

## Test counts

Entry: 742 lib + 757 bin + 128 ui_network + 84 ui_logs + 107 keys + 108 mouse + 53 source_select_help + 7 bugs + 157 network_parser + 14 ws_connect + 1 ws_direct.

Exit: same + UI-015 custom-palette test (+1 in lib). All characterization fences green; UI-042 still green (from Step 3.8).

## Exit gates

- ✅ All `src/ui/**/*.rs` < 500 lines (largest: 493 in `mock_rules.rs`, out of scope)
- ✅ All characterization tests green
- ✅ UI-012 rename complete (`source_select` → `device_picker` in all production code; historical audit/journal references preserved)
- ✅ `cargo clippy --all-targets -- -D warnings` clean
- ✅ `cargo fmt -- --check` clean
- ✅ `cargo test --all` green
