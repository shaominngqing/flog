//! Shared color palette for the JSON viewer.
//!
//! Both the tree renderer (`render.rs`) and the raw-text colorizer
//! (`colorize.rs`) consume the same palette — defined once here so a
//! palette tweak doesn't have to be mirrored across two files.

use ratatui::style::Color;

use super::super::{BLUE, GREEN, LAVENDER, MAUVE, OVERLAY0, PEACH, PINK, SAPPHIRE, SURFACE0, TEAL, YELLOW};

pub(super) const STR_COLOR: Color = GREEN;
pub(super) const NUM_COLOR: Color = PEACH;
pub(super) const BOOL_COLOR: Color = PINK;
pub(super) const NULL_COLOR: Color = OVERLAY0;
pub(super) const COMMA_COLOR: Color = SURFACE0;
pub(super) const FOLD_COLOR: Color = OVERLAY0;

pub(super) const DEPTH_COLORS: [Color; 6] = [MAUVE, BLUE, TEAL, YELLOW, SAPPHIRE, LAVENDER];
pub(super) const DEPTH_BRACE: [Color; 6] = [
    Color::Rgb(110, 115, 141),
    Color::Rgb(100, 105, 131),
    Color::Rgb(90, 95, 121),
    Color::Rgb(80, 85, 111),
    Color::Rgb(73, 77, 100),
    Color::Rgb(54, 58, 79),
];

pub(super) fn key_color(depth: usize) -> Color {
    DEPTH_COLORS[depth % DEPTH_COLORS.len()]
}

pub(super) fn brace_color(depth: usize) -> Color {
    DEPTH_BRACE[depth % DEPTH_BRACE.len()]
}
