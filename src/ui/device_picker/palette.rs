//! Catppuccin Macchiato palette constants used by the device picker.
//! Kept module-local so the picker's render output is stable even if
//! the top-level palette module is tuned.

use ratatui::style::Color;

pub(super) const BASE: Color = Color::Rgb(36, 39, 58);
pub(super) const MANTLE: Color = Color::Rgb(30, 32, 48);
pub(super) const SURFACE0: Color = Color::Rgb(54, 58, 79);
pub(super) const SURFACE1: Color = Color::Rgb(73, 77, 100);
pub(super) const OVERLAY0: Color = Color::Rgb(110, 115, 141);
pub(super) const TEXT: Color = Color::Rgb(202, 211, 245);
pub(super) const SUBTEXT0: Color = Color::Rgb(165, 173, 203);
pub(super) const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
pub(super) const TEAL: Color = Color::Rgb(139, 213, 202);
pub(super) const GREEN: Color = Color::Rgb(166, 218, 149);
