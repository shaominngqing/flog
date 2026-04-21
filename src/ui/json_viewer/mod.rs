//! JSON viewer — AST-based tree display for structured JSON.
//!
//! Submodules:
//! - `tree`     — parse text into a flat arena tree.
//! - `state`    — per-tree fold state.
//! - `render`   — depth-aware rendering with click hit-testing.
//! - `colorize` — raw-text JSON syntax highlight (independent).

mod colorize;
mod palette;
mod render;
mod state;
mod tree;

pub use colorize::colorize_json_text;

// ── Legacy shims ─────────────────────────────────────────────────────────
// These keep the existing callers compiling until Task 8 migrates them.
// Delete once no references remain.

use std::collections::HashSet;

pub struct FmtLine {
    pub text: String,
    pub depth: usize,
    pub close_line: Option<usize>,
}

#[derive(Default, Clone)]
pub struct JsonViewerState {
    pub collapsed: HashSet<usize>,
    pub foldable: HashSet<usize>,
    pub row_to_source: Vec<usize>,
    pub total_lines: usize,
}

pub fn bracket_format(_text: &str) -> Vec<FmtLine> {
    Vec::new()
}

pub fn init_state(_fmt_lines: &[FmtLine], _auto_expand_depth: usize) -> JsonViewerState {
    JsonViewerState::default()
}

pub fn toggle_fold(_state: &mut JsonViewerState, _source_line: usize) -> bool {
    false
}

pub fn render_json(
    _fmt_lines: &[FmtLine],
    _state: &mut JsonViewerState,
    _scroll: usize,
    _max_lines: usize,
    _max_width: usize,
) -> Vec<ratatui::text::Line<'static>> {
    Vec::new()
}
