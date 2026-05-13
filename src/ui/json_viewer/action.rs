//! Action types for interactive hot regions in the JSON viewer.
//!
//! Each rendered row owns zero or more `JsonHotRegion`s — non-overlapping
//! column ranges, each mapped to a `JsonAction`. The detect phase looks
//! up (line_idx, x) in this table; the apply phase executes the action.

use std::ops::Range;

/// Actions associated with interactive hot regions in the JSON viewer.
///
/// Future variants (`CopyNode`, `OpenUrl`, `ExpandFullValue`) are defined now
/// so later tasks can wire them up without changing the type.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // CopyNode/OpenUrl/ExpandFullValue used in upcoming tasks
pub enum JsonAction {
    /// Toggle fold state for a container node.
    ToggleFold(u32),
    /// Copy the subtree rooted at this node as pretty JSON.
    CopyNode(u32),
    /// Open this URL in the system browser. Carries the FULL URL even when
    /// the displayed text is truncated with `…`.
    OpenUrl(String),
    /// Show the full string value of this leaf node in an overlay.
    ExpandFullValue(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonHotRegion {
    pub range: Range<u16>,
    pub action: JsonAction,
}
