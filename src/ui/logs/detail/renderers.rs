//! Per-section renderers. Each is a small, independently-testable unit that
//! turns a `Section` into a list of styled rows.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::DetailState;
use crate::ui::json_viewer;

use super::section::{RenderRow, Section, SectionRenderer};

// Catppuccin Macchiato palette — match the rest of the panel (kept local so
// the module doesn't leak colors back into the chrome file).
const TEXT: Color = Color::Rgb(202, 211, 245);
const OVERLAY0: Color = Color::Rgb(110, 115, 141);
const SURFACE0: Color = Color::Rgb(54, 58, 79);
const TEAL: Color = Color::Rgb(139, 213, 202);
const SAPPHIRE: Color = Color::Rgb(125, 196, 228);
const YELLOW: Color = Color::Rgb(238, 212, 159);
const RED: Color = Color::Rgb(237, 135, 150);
const MAUVE: Color = Color::Rgb(198, 160, 246);

fn row(line: Line<'static>) -> RenderRow {
    RenderRow {
        line,
        click_target: None,
    }
}

// ── Prose ──

pub struct ProseRenderer;

impl SectionRenderer for ProseRenderer {
    fn render(&self, section: &Section, inner_w: usize, _state: &mut DetailState) -> Vec<RenderRow> {
        let Section::Prose(text) = section else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // Prose inside the detail body may still carry `Error: ...` hints from
        // `LogEntry::full_message()`. Color just the leading keyword red so
        // the eye lands on it without having to read every line.
        for wl in crate::ui::wrap_multiline(text, inner_w, 500) {
            out.push(row(style_prose_line(wl)));
        }
        out
    }
}

fn style_prose_line(text: String) -> Line<'static> {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed
        .strip_prefix("Error:")
        .or_else(|| trimmed.strip_prefix("Exception:"))
    {
        let indent_len = text.len() - trimmed.len();
        let indent: String = text.chars().take(indent_len).collect();
        let keyword_len = trimmed.len() - rest.len();
        let keyword: String = trimmed.chars().take(keyword_len).collect();
        return Line::from(vec![
            Span::styled(indent, Style::default().fg(TEXT)),
            Span::styled(
                keyword,
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            ),
            Span::styled(rest.to_string(), Style::default().fg(RED)),
        ]);
    }
    Line::from(Span::styled(text, Style::default().fg(TEXT)))
}

// ── Heading ──

pub struct HeadingRenderer;

impl SectionRenderer for HeadingRenderer {
    fn render(&self, section: &Section, inner_w: usize, _state: &mut DetailState) -> Vec<RenderRow> {
        let Section::Heading(text) = section else {
            return Vec::new();
        };
        // Produce a divider line like `── Stack Trace ──────────────────`
        // padded with `─` out to panel width; label keeps MAUVE, rules use SURFACE0.
        let label = text.trim_matches(['─', ' ']);
        let label_span = Span::styled(
            format!(" {} ", label),
            Style::default()
                .fg(MAUVE)
                .add_modifier(Modifier::BOLD),
        );
        let left = Span::styled("── ", Style::default().fg(SURFACE0));
        let label_w = 3 + label.chars().count() + 2 + 3;
        let right_w = inner_w.saturating_sub(label_w);
        let right = Span::styled(
            "─".repeat(right_w.max(3)),
            Style::default().fg(SURFACE0),
        );
        vec![
            row(Line::from("")),
            row(Line::from(vec![left, label_span, right])),
        ]
    }
}

// ── Stack Trace ──

pub struct StackRenderer;

impl SectionRenderer for StackRenderer {
    fn render(&self, section: &Section, inner_w: usize, _state: &mut DetailState) -> Vec<RenderRow> {
        let Section::StackTrace(text) = section else {
            return Vec::new();
        };
        let collapsed = crate::domain::entry::collapse_stack_frames(text);
        let mut out = Vec::new();
        for raw in collapsed {
            out.push(row(style_stack_line(&raw, inner_w)));
        }
        out
    }
}

fn style_stack_line(line: &str, _inner_w: usize) -> Line<'static> {
    // Async markers — italicized, dim.
    if line.trim() == "<asynchronous suspension>" {
        return Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(OVERLAY0).add_modifier(Modifier::ITALIC),
        ));
    }

    // Collapsed "× N" frames from `collapse_stack_frames` — yellow count suffix.
    if let Some((head, tail)) = line.split_once('×') {
        return Line::from(vec![
            Span::styled(head.to_string(), Style::default().fg(TEAL)),
            Span::styled(
                format!("×{}", tail),
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
        ]);
    }

    // Typical frame: `#N      ClassName.method (package:app/foo.dart:25:3)`
    // Spans: [# + number] dim, [gap + function] TEAL, [(package:...)] SAPPHIRE
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        let indent_len = line.len() - trimmed.len();
        let indent: String = line.chars().take(indent_len).collect();

        // hash + digits
        let hash_digits_end = 1 + trimmed[1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .count();
        let (hash_part, after_num) = trimmed.split_at(hash_digits_end);

        // whitespace gap
        let gap_len = after_num
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(|c| c.len_utf8())
            .sum::<usize>();
        let (gap, after_gap) = after_num.split_at(gap_len);

        // function name = everything up to the first '(' (or end)
        let paren_idx = after_gap.find('(');
        let (func, loc) = match paren_idx {
            Some(p) => after_gap.split_at(p),
            None => (after_gap, ""),
        };

        let mut spans = Vec::with_capacity(5);
        if !indent.is_empty() {
            spans.push(Span::raw(indent));
        }
        spans.push(Span::styled(
            hash_part.to_string(),
            Style::default().fg(OVERLAY0),
        ));
        spans.push(Span::raw(gap.to_string()));
        spans.push(Span::styled(func.to_string(), Style::default().fg(TEAL)));
        if !loc.is_empty() {
            spans.push(Span::styled(loc.to_string(), Style::default().fg(SAPPHIRE)));
        }
        return Line::from(spans);
    }

    Line::from(Span::styled(line.to_string(), Style::default().fg(TEXT)))
}

// ── JSON ──
//
// JsonRenderer is a thin adapter over `json_viewer::append_render`. It
// returns rows with `click_target` populated so fold-on-click keeps working
// after scroll.

pub struct JsonRenderer;

impl SectionRenderer for JsonRenderer {
    fn render(&self, section: &Section, inner_w: usize, state: &mut DetailState) -> Vec<RenderRow> {
        let Section::Json { value, .. } = section else {
            return Vec::new();
        };
        let tree = json_viewer::Tree::from_value(value);
        // Re-init fold state if the node count changed (new entry / different payload).
        if state.viewer_state.expanded.len() != tree.nodes.len() {
            state.viewer_state = json_viewer::init_state(&tree, 1);
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        let mut click_map: Vec<Option<(String, u32)>> = Vec::new();
        json_viewer::append_render(
            &mut lines,
            &mut click_map,
            &tree,
            &state.viewer_state,
            "log_detail",
            "",
            inner_w,
        );
        state.viewer_tree = Some(tree);

        lines
            .into_iter()
            .zip(click_map.into_iter())
            .map(|(line, slot)| RenderRow {
                line,
                click_target: slot.map(|(_, id)| id),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::DetailState;

    #[test]
    fn prose_renders_single_line() {
        let mut state = DetailState::default();
        let section = Section::Prose("hello world");
        let rows = ProseRenderer.render(&section, 80, &mut state);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn prose_colors_error_prefix() {
        let mut state = DetailState::default();
        let section = Section::Prose("Error: something bad");
        let rows = ProseRenderer.render(&section, 80, &mut state);
        assert_eq!(rows.len(), 1);
        // Must have split into multiple spans (keyword + rest).
        let line = &rows[0].line;
        assert!(line.spans.len() >= 2);
    }

    #[test]
    fn stack_renderer_splits_frame_into_spans() {
        let mut state = DetailState::default();
        let section = Section::StackTrace(
            "#0      Foo.bar (package:app/foo.dart:25:3)\n<asynchronous suspension>\n#1      Baz.qux (package:app/baz.dart:1:1)"
        );
        let rows = StackRenderer.render(&section, 80, &mut state);
        assert_eq!(rows.len(), 3);
        // Frame 0 should produce multi-span line (#0 / gap / func / loc).
        assert!(rows[0].line.spans.len() >= 3);
        // Async marker is a single styled span.
        assert_eq!(rows[1].line.spans.len(), 1);
    }

    #[test]
    fn stack_renderer_handles_collapsed_row() {
        let mut state = DetailState::default();
        let section = Section::StackTrace(
            "#0      Foo._emit (package:app/foo.dart:25:3)\n\
             #1      Foo._emit (package:app/foo.dart:25:3)\n\
             #2      Foo._emit (package:app/foo.dart:25:3)"
        );
        let rows = StackRenderer.render(&section, 80, &mut state);
        // collapse_stack_frames compresses 3 identical frames into 1 "× 3" row.
        assert_eq!(rows.len(), 1);
        let text = rows[0].line.spans.iter().map(|s| s.content.clone()).collect::<String>();
        assert!(text.contains("× 3"));
    }

    #[test]
    fn heading_renderer_emits_blank_then_rule() {
        let mut state = DetailState::default();
        let section = Section::Heading("── Stack Trace ──");
        let rows = HeadingRenderer.render(&section, 40, &mut state);
        assert_eq!(rows.len(), 2);
    }
}
