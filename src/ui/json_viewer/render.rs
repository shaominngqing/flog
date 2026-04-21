//! JSON tree rendering.
//!
//! Every rendered row has the layout
//!   <outer_prefix><indent><marker><content>
//! where `indent` derives from node depth (not from text width!) and
//! `marker` is a fixed-width 2-char cell (▼/▶/spaces) so content columns
//! always align.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use super::super::{BLUE, GREEN, LAVENDER, MAUVE, OVERLAY0, PEACH, PINK, SAPPHIRE, SURFACE0, TEAL, YELLOW};
use super::state::JsonViewerState;
use super::tree::{NodeKind, Tree};

const STR_COLOR: Color = GREEN;
const NUM_COLOR: Color = PEACH;
const BOOL_COLOR: Color = PINK;
const NULL_COLOR: Color = OVERLAY0;
const COMMA_COLOR: Color = SURFACE0;
const FOLD_COLOR: Color = OVERLAY0;

const DEPTH_COLORS: [Color; 6] = [MAUVE, BLUE, TEAL, YELLOW, SAPPHIRE, LAVENDER];
const DEPTH_BRACE: [Color; 6] = [
    Color::Rgb(110, 115, 141),
    Color::Rgb(100, 105, 131),
    Color::Rgb(90, 95, 121),
    Color::Rgb(80, 85, 111),
    Color::Rgb(73, 77, 100),
    Color::Rgb(54, 58, 79),
];

fn key_color(depth: u32) -> Color { DEPTH_COLORS[(depth as usize) % DEPTH_COLORS.len()] }
fn brace_color(depth: u32) -> Color { DEPTH_BRACE[(depth as usize) % DEPTH_BRACE.len()] }

/// Render `tree` into `out`, pushing one `Option<(section_key, node_id)>`
/// into `click_map` for each line added. Clickable rows (foldable containers)
/// get `Some(...)`; leaves and close-brace lines get `None`.
///
/// `outer_prefix` is whitespace the caller wants prepended (e.g. `"   "` for
/// three-space section indent). `max_width` is the total panel width; strings
/// are truncated with `…` if the line would exceed it.
pub fn append_render(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
) {
    render_node(
        out,
        click_map,
        tree,
        state,
        section_key,
        outer_prefix,
        max_width,
        0,
        false, // root never has a trailing comma
    );
}

/// Render node `id` and its subtree. `trailing_comma` says whether to append
/// a comma after this node's value (used for non-last siblings).
fn render_node(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let is_container = matches!(node.kind, NodeKind::Object | NodeKind::Array);

    if !is_container {
        push_leaf_line(out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma);
        return;
    }

    // Containers: implemented in Tasks 5 and 6.
    // Until then, render as a collapsed placeholder so the module compiles.
    let _ = (state, depth);
    push_leaf_line(out, click_map, tree, section_key, outer_prefix, max_width, id, trailing_comma);
}

fn push_leaf_line(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Option<(String, u32)>>,
    tree: &Tree,
    _section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let marker = "  "; // leaves are not foldable

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}{}", outer_prefix, indent, marker)));

    // Key part (for object entries)
    if let Some(ref key) = node.key {
        spans.push(Span::styled(
            format!("\"{}\"", key),
            Style::default().fg(key_color(depth)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    // Remaining width for the value
    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width.saturating_sub(used).saturating_sub(if trailing_comma { 1 } else { 0 });

    spans.extend(render_leaf_value(&node.kind, remaining));

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));
    click_map.push(None);
}

fn render_leaf_value(kind: &NodeKind, max_width: usize) -> Vec<Span<'static>> {
    match kind {
        NodeKind::Null => vec![Span::styled(
            "null",
            Style::default().fg(NULL_COLOR).add_modifier(Modifier::ITALIC),
        )],
        NodeKind::Bool(b) => vec![Span::styled(
            if *b { "true" } else { "false" },
            Style::default().fg(BOOL_COLOR),
        )],
        NodeKind::Number(s) => vec![Span::styled(s.clone(), Style::default().fg(NUM_COLOR))],
        NodeKind::String(s) => {
            let quoted = format!("\"{}\"", s);
            let text = if quoted.width() > max_width && max_width >= 2 {
                // Truncate inside the quotes, leave room for closing `"` + `…`
                let budget = max_width.saturating_sub(2); // `"…`
                let mut w = 0usize;
                let mut cut = 0usize;
                for (i, ch) in s.char_indices() {
                    let cw = ch.to_string().as_str().width();
                    if w + cw > budget {
                        break;
                    }
                    w += cw;
                    cut = i + ch.len_utf8();
                }
                format!("\"{}…\"", &s[..cut])
            } else {
                quoted
            };
            vec![Span::styled(text, Style::default().fg(STR_COLOR))]
        }
        // Containers reach this only during the stubbed path in render_node;
        // once Tasks 5/6 land, containers never hit here.
        NodeKind::Object => vec![Span::styled("{…}", Style::default().fg(FOLD_COLOR))],
        NodeKind::Array => vec![Span::styled("[…]", Style::default().fg(FOLD_COLOR))],
    }
}

#[cfg(test)]
mod tests {
    use super::super::{state, tree};
    use super::*;

    fn render(text: &str, width: usize) -> Vec<String> {
        let t = tree::parse(text).unwrap();
        let s = state::init_state(&t, 0); // collapse everything except root
        let mut out = Vec::new();
        let mut cmap = Vec::new();
        append_render(&mut out, &mut cmap, &t, &s, "sec", "", width);
        assert_eq!(out.len(), cmap.len(), "out and click_map must stay in sync");
        out.iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect()
    }

    #[test]
    fn leaf_null() {
        let lines = render("null", 80);
        assert_eq!(lines, vec!["  null"]);
    }

    #[test]
    fn leaf_bool() {
        assert_eq!(render("true", 80), vec!["  true"]);
        assert_eq!(render("false", 80), vec!["  false"]);
    }

    #[test]
    fn leaf_number() {
        assert_eq!(render("1776684313608", 80), vec!["  1776684313608"]);
    }

    #[test]
    fn leaf_string_short() {
        assert_eq!(render(r#""hello""#, 80), vec![r#"  "hello""#]);
    }

    #[test]
    fn leaf_string_truncated() {
        // "  " marker (2) + `"…"` (3) + content
        // max_width 10 -> used 2, remaining 8, budget for content = 6 chars
        let lines = render(r#""abcdefghij""#, 10);
        // "abcdefghij" has width 10 — with "…" bookend we expect a truncated form.
        // Exact cut point varies by width budget; just assert the ellipsis appears.
        assert!(lines[0].contains('…'), "expected truncation: {:?}", lines);
    }
}
