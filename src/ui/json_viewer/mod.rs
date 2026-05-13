//! JSON viewer — AST-based tree display for structured JSON.
//!
//! Submodules:
//! - `tree`     — parse text into a flat arena tree.
//! - `state`    — per-tree fold state.
//! - `render`   — depth-aware rendering with click hit-testing.
//! - `colorize` — raw-text JSON syntax highlight (independent).
//! - `action`   — typed hot-region actions for interactive dispatch.

mod action;
mod colorize;
mod palette;
mod render;
mod state;
mod tree;

pub use action::{JsonAction, JsonHotRegion};
pub use colorize::colorize_json_text;
pub use render::append_render;
pub use state::{init_state, toggle, JsonViewerState};
pub use tree::{NodeKind, Tree};
