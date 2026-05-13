//! Body-of-detail modelled as an ordered list of `Section`s, each rendered by
//! its own `SectionRenderer`. This keeps `detail/mod.rs` focused on chrome
//! (header, scrollbar, block) and lets new content types (stack traces,
//! network payloads, SSE chunks) plug in by adding a variant + renderer.

use ratatui::text::Line;
use serde_json::Value;
use std::ops::Range;

use crate::app::DetailState;

/// A semantically-typed chunk of the detail body.
///
/// `build_sections` produces a flat `Vec<Section>` from the entry's full
/// message; each variant carries just the text it owns (never &mut state).
pub enum Section<'a> {
    /// Section header like `── Stack Trace ──`.
    Heading(&'a str),
    /// Plain prose — wrapped by `wrap_multiline`, default text color.
    Prose(&'a str),
    /// Dart stack trace body (frames + `<asynchronous suspension>` markers).
    /// Collapsed `× N` rows are rendered single-row, matching list view.
    StackTrace(&'a str),
    /// Structured JSON discovered inside the message. `range` is the byte
    /// range within the full message (used by caller to trim prefix/suffix).
    Json {
        value: Value,
        #[allow(dead_code)] // reserved for future highlight-source-region support
        range: Range<usize>,
    },
}

/// Output slot for a renderer: a visual line plus zero or more interactive hot regions.
///
/// `hot_regions` carries zero or more interactive regions for this row
/// (fold toggle, copy, URL, expand). Empty for non-interactive rows.
pub struct RenderRow {
    pub line: Line<'static>,
    pub hot_regions: Vec<crate::ui::json_viewer::JsonHotRegion>,
}

/// Renderers take `&mut DetailState` so stateful renderers (JSON fold,
/// future stack-collapse expansion) can read/update their own sub-state.
pub trait SectionRenderer {
    fn render(&self, section: &Section, inner_w: usize, state: &mut DetailState) -> Vec<RenderRow>;
}

// ══════════════════════════════════════
//  Section Discovery
// ══════════════════════════════════════

/// Split `full_msg` into an ordered list of sections.
///
/// Current rules (intentionally narrow — grow as more content types land):
///   1. If `structured_parser::find_and_parse` hits, split into
///      `[Prose(prefix), Json, Prose(suffix)]`.
///   2. Otherwise walk the text line-by-line: `── XYZ ──` lines become
///      `Heading`, runs of `#N ...` frames (with any interleaved
///      `<asynchronous suspension>` markers) become `StackTrace`,
///      everything else accumulates into `Prose`.
pub fn build_sections(full_msg: &str) -> Vec<Section<'_>> {
    if let Some((start, end, value)) = crate::domain::structured_parser::find_and_parse(full_msg) {
        let mut out: Vec<Section<'_>> = Vec::new();
        let prefix = full_msg[..start].trim_end();
        if !prefix.is_empty() {
            out.extend(split_text_sections(prefix));
        }
        out.push(Section::Json {
            value,
            range: start..end,
        });
        let suffix = full_msg[end..].trim();
        if !suffix.is_empty() {
            out.extend(split_text_sections(suffix));
        }
        return out;
    }
    split_text_sections(full_msg)
}

/// Walk `text` and emit Heading / StackTrace / Prose sections by line class.
fn split_text_sections(text: &str) -> Vec<Section<'_>> {
    let mut out: Vec<Section<'_>> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut prose_start = 0usize;

    while i < bytes.len() {
        // locate end-of-line (exclusive) and start of next (after '\n', may equal len)
        let line_end = bytes[i..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| i + p)
            .unwrap_or(bytes.len());
        let next = if line_end < bytes.len() {
            line_end + 1
        } else {
            line_end
        };
        let line = &text[i..line_end];

        if is_heading(line) {
            flush_prose(text, prose_start, i, &mut out);
            out.push(Section::Heading(line.trim_matches(['\n', ' '])));
            prose_start = next;
            i = next;
            continue;
        }

        if is_stack_frame(line) || is_async_marker(line) {
            flush_prose(text, prose_start, i, &mut out);
            let stack_start = i;
            // consume consecutive frame / async-marker lines
            let mut cursor = next;
            let mut scan = next;
            while scan < bytes.len() {
                let le = bytes[scan..]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map(|p| scan + p)
                    .unwrap_or(bytes.len());
                let nl_end = if le < bytes.len() { le + 1 } else { le };
                let l = &text[scan..le];
                if is_stack_frame(l) || is_async_marker(l) {
                    cursor = nl_end;
                    scan = nl_end;
                } else {
                    break;
                }
            }
            // trim trailing newline inside the stack slice for cleanliness
            let stack_end = cursor.min(bytes.len());
            let stack_text = text[stack_start..stack_end].trim_end_matches('\n');
            out.push(Section::StackTrace(stack_text));
            prose_start = cursor;
            i = cursor;
            continue;
        }

        i = next;
    }
    flush_prose(text, prose_start, bytes.len(), &mut out);
    out
}

fn flush_prose<'a>(text: &'a str, start: usize, end: usize, out: &mut Vec<Section<'a>>) {
    if start >= end {
        return;
    }
    let slice = text[start..end].trim_matches('\n');
    if !slice.is_empty() {
        out.push(Section::Prose(slice));
    }
}

fn is_heading(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("──") && t.ends_with("──")
}

fn is_stack_frame(line: &str) -> bool {
    let t = line.trim_start();
    if !t.starts_with('#') {
        return false;
    }
    let rest = t[1..].trim_start_matches(|c: char| c.is_ascii_digit());
    rest.starts_with(char::is_whitespace)
}

fn is_async_marker(line: &str) -> bool {
    let t = line.trim();
    t == "<asynchronous suspension>"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_only() {
        let s = "just some text\nsecond line";
        let sections = build_sections(s);
        assert_eq!(sections.len(), 1);
        assert!(matches!(&sections[0], Section::Prose(t) if *t == s));
    }

    #[test]
    fn splits_heading_and_stack() {
        let s = "leading prose\n── Stack Trace ──\n\n#0 foo (a.dart:1:1)\n#1 bar (b.dart:2:2)\n<asynchronous suspension>\n#2 baz (c.dart:3:3)";
        let sections = build_sections(s);
        let kinds: Vec<&str> = sections
            .iter()
            .map(|sec| match sec {
                Section::Prose(_) => "prose",
                Section::Heading(_) => "heading",
                Section::StackTrace(_) => "stack",
                Section::Json { .. } => "json",
            })
            .collect();
        assert_eq!(kinds, vec!["prose", "heading", "stack"]);

        if let Section::StackTrace(stack) = &sections[2] {
            assert!(stack.contains("#0"));
            assert!(stack.contains("<asynchronous suspension>"));
            assert!(stack.contains("#2"));
        } else {
            panic!("expected stacktrace section");
        }
    }

    #[test]
    fn error_heading_then_prose() {
        let s = "── Error ──\n\nSomething broke";
        let sections = build_sections(s);
        assert_eq!(sections.len(), 2);
        assert!(matches!(&sections[0], Section::Heading(t) if t.contains("Error")));
        assert!(matches!(&sections[1], Section::Prose(t) if t.contains("Something broke")));
    }

    #[test]
    fn json_path_produces_prose_json_prose() {
        let s = "prefix text {code: 0, name: bar} trailing";
        let sections = build_sections(s);
        // prefix prose + json + suffix prose (suffix after trim)
        let kinds: Vec<&str> = sections
            .iter()
            .map(|sec| match sec {
                Section::Prose(_) => "prose",
                Section::Heading(_) => "heading",
                Section::StackTrace(_) => "stack",
                Section::Json { .. } => "json",
            })
            .collect();
        assert!(kinds.contains(&"json"));
        assert_eq!(kinds.first(), Some(&"prose"));
    }
}
