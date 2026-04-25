//! Help overlay — per-view content sections.
//!
//! Each submodule returns a `Vec<Line<'static>>` the top-level `draw_help`
//! concatenates in order. Splitting keeps `mod.rs` focused on layout and
//! shared primitives while each view's documentation stays co-located.

pub(super) mod logs;
pub(super) mod network;
