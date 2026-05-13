//! Per-line emitters for the JSON tree render loop.
//!
//! Every rendered row has the layout
//!   `<outer_prefix><indent><marker><content>`
//! where `indent` derives from node depth (not from text width!) and
//! `marker` is a fixed-width 2-char cell (▼/▶/spaces) so content columns
//! always align.

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

use crate::ui::{sanitize_for_cell, BLUE};

use super::super::action::{JsonAction, JsonHotRegion};
use super::super::palette::{
    brace_color, key_color, BOOL_COLOR, COMMA_COLOR, FOLD_COLOR, NULL_COLOR, NUM_COLOR, STR_COLOR,
};
use super::super::state::JsonViewerState;
use super::super::tree::{NodeKind, Tree};
use super::summaries::summarize_container;

/// Render node `id` and its subtree. `trailing_comma` says whether to append
/// a comma after this node's value (used for non-last siblings).
#[allow(clippy::too_many_arguments)]
pub(super) fn render_node(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    state: &JsonViewerState,
    section_key: &str,
    outer_prefix: &str,
    max_width: usize,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let is_container = matches!(node.kind, NodeKind::Object | NodeKind::Array);

    if !is_container {
        push_leaf_line(
            out,
            click_map,
            tree,
            section_key,
            outer_prefix,
            max_width,
            id,
            trailing_comma,
        );
        return;
    }

    let expanded = state.is_expanded(id);
    if !expanded || node.children.is_empty() {
        push_container_collapsed(
            out,
            click_map,
            tree,
            section_key,
            outer_prefix,
            max_width,
            id,
            trailing_comma,
        );
        return;
    }

    // Recursion safety: `tree::parse` uses `serde_json::from_str` which has
    // a default nesting cap of 128. Any JSON that parses is safe to recurse on.
    push_container_opener(out, click_map, tree, section_key, outer_prefix, id);
    let child_count = node.children.len();
    for (i, &cid) in node.children.iter().enumerate() {
        let child_trailing = i + 1 < child_count;
        render_node(
            out,
            click_map,
            tree,
            state,
            section_key,
            outer_prefix,
            max_width,
            cid,
            child_trailing,
        );
    }
    push_container_closer(out, click_map, tree, outer_prefix, id, trailing_comma);
}

#[allow(clippy::too_many_arguments)]
fn push_leaf_line(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
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
        // UI-046: JSON object keys come from user data too.
        spans.push(Span::styled(
            format!("\"{}\"", sanitize_for_cell(key)),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    // Remaining width for the value
    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width
        .saturating_sub(used)
        .saturating_sub(if trailing_comma { 1 } else { 0 });

    spans.extend(render_leaf_value(&node.kind, remaining));

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));
    click_map.push(Vec::new());
}

fn render_leaf_value(kind: &NodeKind, max_width: usize) -> Vec<Span<'static>> {
    match kind {
        NodeKind::Null => vec![Span::styled(
            "null",
            Style::default()
                .fg(NULL_COLOR)
                .add_modifier(Modifier::ITALIC),
        )],
        NodeKind::Bool(b) => vec![Span::styled(
            if *b { "true" } else { "false" },
            Style::default().fg(BOOL_COLOR),
        )],
        NodeKind::Number(s) => vec![Span::styled(s.clone(), Style::default().fg(NUM_COLOR))],
        NodeKind::String(s) => {
            // UI-046: JSON string values come from arbitrary user data
            // (WS frames, HTTP bodies). Sanitize before measuring and
            // truncating so the grapheme scan sees the same bytes that
            // will later land in the Span.
            let s_safe = sanitize_for_cell(s);
            let quoted = format!("\"{}\"", s_safe);
            let text = if quoted.width() > max_width && max_width >= 3 {
                // Output format is `"<content>…"` — three fixed cells
                // (open quote, ellipsis, close quote).
                let budget = max_width.saturating_sub(3);
                let mut w = 0usize;
                let mut cut = 0usize;
                for (i, ch) in s_safe.char_indices() {
                    let cw = ch.to_string().as_str().width();
                    if w + cw > budget {
                        break;
                    }
                    w += cw;
                    cut = i + ch.len_utf8();
                }
                format!("\"{}…\"", &s_safe[..cut])
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

#[allow(clippy::too_many_arguments)]
fn push_container_collapsed(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
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
    let empty = node.children.is_empty();

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));

    // Marker. Empty containers are not foldable — show blank marker.
    if empty {
        spans.push(Span::raw("  "));
    } else {
        spans.push(Span::styled("▶ ", Style::default().fg(BLUE)));
    }

    // Key
    if let Some(ref key) = node.key {
        // UI-046: JSON object keys come from user data too.
        spans.push(Span::styled(
            format!("\"{}\"", sanitize_for_cell(key)),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }

    let used: usize = spans.iter().map(|s| s.content.as_ref().width()).sum();
    let remaining = max_width
        .saturating_sub(used)
        .saturating_sub(if trailing_comma { 1 } else { 0 });

    if empty {
        // Empty: just `{}` or `[]`
        let (open, close) = match node.kind {
            NodeKind::Object => ("{", "}"),
            NodeKind::Array => ("[", "]"),
            _ => unreachable!(),
        };
        spans.push(Span::styled(
            format!("{}{}", open, close),
            Style::default().fg(brace_color(depth as usize)),
        ));
    } else {
        spans.extend(summarize_container(tree, id, remaining));
    }

    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }

    out.push(Line::from(spans));
    click_map.push(if empty {
        Vec::new()
    } else {
        vec![JsonHotRegion {
            range: 0..u16::MAX,
            action: JsonAction::ToggleFold(id),
        }]
    });
}

fn push_container_opener(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    _section_key: &str,
    outer_prefix: &str,
    id: u32,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let open = match node.kind {
        NodeKind::Object => "{",
        NodeKind::Array => "[",
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));
    spans.push(Span::styled("▼ ", Style::default().fg(BLUE)));
    if let Some(ref key) = node.key {
        // UI-046: JSON object keys come from user data too.
        spans.push(Span::styled(
            format!("\"{}\"", sanitize_for_cell(key)),
            Style::default().fg(key_color(depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    spans.push(Span::styled(
        open.to_string(),
        Style::default().fg(brace_color(depth as usize)),
    ));

    out.push(Line::from(spans));
    click_map.push(vec![JsonHotRegion {
        range: 0..u16::MAX,
        action: JsonAction::ToggleFold(id),
    }]);
}

fn push_container_closer(
    out: &mut Vec<Line<'static>>,
    click_map: &mut Vec<Vec<JsonHotRegion>>,
    tree: &Tree,
    outer_prefix: &str,
    id: u32,
    trailing_comma: bool,
) {
    let node = tree.node(id);
    let depth = node.depth;
    let indent = "  ".repeat(depth as usize);
    let close = match node.kind {
        NodeKind::Object => "}",
        NodeKind::Array => "]",
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw(format!("{}{}", outer_prefix, indent)));
    spans.push(Span::raw("  ")); // marker column — closer is not foldable
    spans.push(Span::styled(
        close.to_string(),
        Style::default().fg(brace_color(depth as usize)),
    ));
    if trailing_comma {
        spans.push(Span::styled(",", Style::default().fg(COMMA_COLOR)));
    }
    out.push(Line::from(spans));
    click_map.push(Vec::new());
}
