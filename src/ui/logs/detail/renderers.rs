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
    fn render(
        &self,
        section: &Section,
        inner_w: usize,
        _state: &mut DetailState,
    ) -> Vec<RenderRow> {
        let Section::Prose(text) = section else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for wl in crate::ui::wrap_multiline(text, inner_w, 500) {
            out.push(row(style_prose_line(wl)));
        }
        out
    }
}

/// Style a single wrapped prose line.
///
/// Delegates to `auto_highlight` for the same HTTP method / status code /
/// URL / duration coloring used by the list view. Lines whose trimmed text
/// begins with `Error:` / `Exception:` get an extra RED+bold keyword span
/// (highlight rules alone miss the `:`), then the remainder still flows
/// through `auto_highlight` so any embedded URL/status/method is colored.
fn style_prose_line(text: String) -> Line<'static> {
    let base = Style::default().fg(TEXT);
    let trimmed = text.trim_start();

    let keyword = if trimmed.starts_with("Error:") {
        Some("Error:")
    } else if trimmed.starts_with("Exception:") {
        Some("Exception:")
    } else {
        None
    };

    if let Some(kw) = keyword {
        let indent_len = text.len() - trimmed.len();
        let indent: String = text.chars().take(indent_len).collect();
        let rest_start = indent_len + kw.len();
        let rest: String = text.chars().skip(rest_start).collect();
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(3);
        if !indent.is_empty() {
            spans.push(Span::styled(indent, base));
        }
        spans.push(Span::styled(
            kw.to_string(),
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        ));
        spans.extend(crate::ui::logs::highlight::auto_highlight(&rest, base));
        return Line::from(spans);
    }

    Line::from(crate::ui::logs::highlight::auto_highlight(&text, base))
}

// ── Heading ──

pub struct HeadingRenderer;

impl SectionRenderer for HeadingRenderer {
    fn render(
        &self,
        section: &Section,
        inner_w: usize,
        _state: &mut DetailState,
    ) -> Vec<RenderRow> {
        let Section::Heading(text) = section else {
            return Vec::new();
        };
        // Produce a divider line like `── Stack Trace ──────────────────`
        // padded with `─` out to panel width; label keeps MAUVE, rules use SURFACE0.
        let label = text.trim_matches(['─', ' ']);
        let label_span = Span::styled(
            format!(" {} ", label),
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        );
        let left = Span::styled("── ", Style::default().fg(SURFACE0));
        let label_w = 3 + label.chars().count() + 2 + 3;
        let right_w = inner_w.saturating_sub(label_w);
        let right = Span::styled("─".repeat(right_w.max(3)), Style::default().fg(SURFACE0));
        vec![
            row(Line::from("")),
            row(Line::from(vec![left, label_span, right])),
        ]
    }
}

// ── Stack Trace ──

pub struct StackRenderer;

impl SectionRenderer for StackRenderer {
    fn render(
        &self,
        section: &Section,
        inner_w: usize,
        _state: &mut DetailState,
    ) -> Vec<RenderRow> {
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

/// Classify a stack frame location by its source origin.
///
/// `dart:*` and `package:flutter/**` are considered SDK/framework noise —
/// rendered extra-dim so the eye can skip past them. Everything else
/// (user code, third-party libraries like dio/hive) keeps full color.
fn is_sdk_frame_loc(loc: &str) -> bool {
    // loc looks like `(dart:io/common.dart:58:9)` or `(package:flutter/src/...)`.
    let inner = loc.trim_start_matches('(');
    inner.starts_with("dart:")
        || inner.starts_with("package:flutter/")
        || inner.starts_with("package:flutter_")
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
    // SDK frames (dart:*, package:flutter*) get rendered in OVERLAY0 across the
    // board so user-code frames visually pop.
    let trimmed = line.trim_start();
    if let Some(stripped) = trimmed.strip_prefix('#') {
        let indent_len = line.len() - trimmed.len();
        let indent: String = line.chars().take(indent_len).collect();

        // hash + digits
        let hash_digits_end = 1 + stripped.chars().take_while(|c| c.is_ascii_digit()).count();
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

        let is_sdk = is_sdk_frame_loc(loc);
        let (hash_fg, func_fg, loc_fg) = if is_sdk {
            // Entire SDK frame dimmed — hash, name, location all OVERLAY0 so
            // runs of framework frames recede into the background.
            (OVERLAY0, OVERLAY0, OVERLAY0)
        } else {
            (OVERLAY0, TEAL, SAPPHIRE)
        };

        let mut spans = Vec::with_capacity(5);
        if !indent.is_empty() {
            spans.push(Span::raw(indent));
        }
        spans.push(Span::styled(
            hash_part.to_string(),
            Style::default().fg(hash_fg),
        ));
        spans.push(Span::raw(gap.to_string()));
        spans.push(Span::styled(func.to_string(), Style::default().fg(func_fg)));
        if !loc.is_empty() {
            spans.push(Span::styled(loc.to_string(), Style::default().fg(loc_fg)));
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
            .zip(click_map)
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
    fn prose_auto_highlights_http_line() {
        let mut state = DetailState::default();
        let section = Section::Prose("GET /api/users 200 (42ms)");
        let rows = ProseRenderer.render(&section, 80, &mut state);
        assert_eq!(rows.len(), 1);
        // auto_highlight must split the line: method + path + status + duration
        // should each land in their own span (4+ tokens, ≥4 spans with gaps).
        assert!(
            rows[0].line.spans.len() >= 4,
            "expected multi-span line, got {} spans",
            rows[0].line.spans.len()
        );
    }

    #[test]
    fn prose_auto_highlights_error_rest() {
        // Even after the Error: keyword, trailing URL / status should be colored.
        let mut state = DetailState::default();
        let section = Section::Prose("Error: request https://api.example.com failed with 500");
        let rows = ProseRenderer.render(&section, 200, &mut state);
        assert_eq!(rows.len(), 1);
        // Expect: keyword span + at least URL + status highlighted in the rest
        assert!(rows[0].line.spans.len() >= 3);
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
             #2      Foo._emit (package:app/foo.dart:25:3)",
        );
        let rows = StackRenderer.render(&section, 80, &mut state);
        // collapse_stack_frames compresses 3 identical frames into 1 "× 3" row.
        assert_eq!(rows.len(), 1);
        let text = rows[0]
            .line
            .spans
            .iter()
            .map(|s| s.content.clone())
            .collect::<String>();
        assert!(text.contains("× 3"));
    }

    #[test]
    fn stack_renderer_dims_sdk_frames() {
        let mut state = DetailState::default();
        let section = Section::StackTrace(
            "#0      _checkForErrorResponse (dart:io/common.dart:58:9)\n\
             #1      MyApp.bootstrap (package:aura_lang_flutter/bootstrap.dart:23:7)\n\
             #2      StatelessWidget.build (package:flutter/src/widgets/framework.dart:400:1)",
        );
        let rows = StackRenderer.render(&section, 80, &mut state);
        assert_eq!(rows.len(), 3);

        // Collect the location-span color from each frame row. User code uses
        // SAPPHIRE; SDK frames use OVERLAY0.
        let loc_colors: Vec<_> = rows
            .iter()
            .map(|r| r.line.spans.last().unwrap().style.fg)
            .collect();
        assert_eq!(loc_colors[0], Some(OVERLAY0)); // dart:* → dim
        assert_eq!(loc_colors[1], Some(SAPPHIRE)); // user package → bright
        assert_eq!(loc_colors[2], Some(OVERLAY0)); // package:flutter → dim
    }

    #[test]
    fn sdk_frame_classifier() {
        assert!(is_sdk_frame_loc("(dart:async/zone.dart:48:47)"));
        assert!(is_sdk_frame_loc("(dart:io/common.dart:58:9)"));
        assert!(is_sdk_frame_loc(
            "(package:flutter/src/widgets/framework.dart:1:1)"
        ));
        assert!(is_sdk_frame_loc(
            "(package:flutter_test/flutter_test.dart:1:1)"
        ));
        assert!(!is_sdk_frame_loc(
            "(package:aura_lang_flutter/bootstrap.dart:23:7)"
        ));
        assert!(!is_sdk_frame_loc("(package:hive/src/backend_vm.dart:81:5)"));
        assert!(!is_sdk_frame_loc("(package:dio/src/dio.dart:10:1)"));
    }

    #[test]
    fn heading_renderer_emits_blank_then_rule() {
        let mut state = DetailState::default();
        let section = Section::Heading("── Stack Trace ──");
        let rows = HeadingRenderer.render(&section, 40, &mut state);
        assert_eq!(rows.len(), 2);
    }
}
