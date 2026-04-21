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

// ── New AST-based API ────────────────────────────────────────────────────
pub use render::append_render;
pub use state::{collapse_all, expand_all, toggle};
pub use tree::{parse, NodeKind, Tree};

// Re-export the real state type under a different name so callers can
// migrate gradually. Once the legacy shim is deleted (Task 8), we rename
// this to `JsonViewerState` and drop the alias.
pub use state::JsonViewerState as AstViewerState;

/// New init_state for the AST viewer. Renamed to avoid clash with the
/// legacy stub until Task 8.
pub fn init_ast_state(tree: &Tree, default_expand_depth: u32) -> AstViewerState {
    state::init_state(tree, default_expand_depth)
}

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
