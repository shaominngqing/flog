//! Observable-feature helpers for ratatui TestBackend buffers.
//!
//! Assertions target semantic facts ("there's a red cell," "this text appears
//! with this fg color") not raw pixel dumps. Phase 3 can refactor render
//! internals without breaking these tests as long as the user-visible
//! behavior stays the same.
#![allow(dead_code)]

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Style};

/// Iterate every (x, y) position in the buffer.
fn positions(buf: &Buffer) -> impl Iterator<Item = (u16, u16)> + '_ {
    let area = buf.area;
    (0..area.height).flat_map(move |y| (0..area.width).map(move |x| (area.x + x, area.y + y)))
}

/// Count cells whose background matches `bg` exactly.
pub fn count_cells_with_bg(buf: &Buffer, bg: Color) -> usize {
    positions(buf)
        .filter(|&(x, y)| buf[(x, y)].bg == bg)
        .count()
}

/// Count cells whose foreground matches `fg` exactly.
pub fn count_cells_with_fg(buf: &Buffer, fg: Color) -> usize {
    positions(buf)
        .filter(|&(x, y)| buf[(x, y)].fg == fg)
        .count()
}

/// Find the first row containing `needle` as a substring of the joined
/// symbols. Returns the 0-based row index (relative to `buf.area.y`).
pub fn find_text_row(buf: &Buffer, needle: &str) -> Option<u16> {
    let area = buf.area;
    for row in 0..area.height {
        let y = area.y + row;
        let joined: String = (0..area.width)
            .map(|col| buf[(area.x + col, y)].symbol())
            .collect::<Vec<_>>()
            .join("");
        if joined.contains(needle) {
            return Some(row);
        }
    }
    None
}

/// Count how many rows contain `needle`.
pub fn count_rows_with_text(buf: &Buffer, needle: &str) -> usize {
    let area = buf.area;
    let mut n = 0;
    for row in 0..area.height {
        let y = area.y + row;
        let joined: String = (0..area.width)
            .map(|col| buf[(area.x + col, y)].symbol())
            .collect::<Vec<_>>()
            .join("");
        if joined.contains(needle) {
            n += 1;
        }
    }
    n
}

/// Dump row `y` (relative to buffer origin) as a single string (for
/// debug output in failing tests).
pub fn row_to_string(buf: &Buffer, y: u16) -> String {
    let area = buf.area;
    let abs_y = area.y + y;
    (0..area.width)
        .map(|col| buf[(area.x + col, abs_y)].symbol().to_string())
        .collect::<Vec<_>>()
        .join("")
}

/// Style of a single cell. `x` and `y` are relative to the buffer origin.
pub fn style_at(buf: &Buffer, x: u16, y: u16) -> Style {
    let area = buf.area;
    buf[(area.x + x, area.y + y)].style()
}

/// Assert buf has at least `min` cells with the given bg. Panics with a
/// diagnostic message including actual count and a sample offending row.
pub fn assert_min_cells_with_bg(buf: &Buffer, bg: Color, min: usize, ctx: &str) {
    let n = count_cells_with_bg(buf, bg);
    if n < min {
        let sample = if buf.area.height > 0 {
            row_to_string(buf, 0)
        } else {
            String::new()
        };
        panic!(
            "{ctx}: expected at least {min} cells with bg {:?}, got {n}. first row: {:?}",
            bg, sample
        );
    }
}

/// Concatenate every cell in the buffer into a single string, rows
/// separated by '\n'. Used for broad "does this text appear anywhere"
/// assertions.
pub fn full_text(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::new();
    for row in 0..area.height {
        if row > 0 {
            out.push('\n');
        }
        let y = area.y + row;
        for col in 0..area.width {
            out.push_str(buf[(area.x + col, y)].symbol());
        }
    }
    out
}

/// Collect every distinct color (fg or bg) present in the buffer — used
/// for "verify palette diversity" characterization tests.
pub fn distinct_colors(buf: &Buffer) -> HashSet<Color> {
    let mut set = HashSet::new();
    for (x, y) in positions(buf) {
        set.insert(buf[(x, y)].fg);
        set.insert(buf[(x, y)].bg);
    }
    set
}
