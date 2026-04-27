//! DevTools-style one-line summaries for collapsed containers.
//!
//! Objects render as `{k: v, k2: v2, …}`; arrays render as
//! `[v, v2, …] (N)` where `N` is the element count. Width-aware: stops
//! emitting preview entries when the budget is exhausted and appends
//! `…` before the closer. Nested containers preview as `{…}` / `[…]`.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use unicode_width::UnicodeWidthStr;

use crate::ui::OVERLAY0;

use super::super::palette::{
    brace_color, key_color, BOOL_COLOR, COMMA_COLOR, FOLD_COLOR, NULL_COLOR, NUM_COLOR, STR_COLOR,
};
use super::super::tree::{NodeKind, Tree};

/// Render a collapsed container's one-line summary.
/// Objects: `{k: v, k2: v2, …}`. Arrays: `[v, v2, …] (<len>)`.
/// Width-aware: stops emitting entries when the budget is exhausted and
/// appends `…` + closer.
pub(super) fn summarize_container(tree: &Tree, id: u32, max_width: usize) -> Vec<Span<'static>> {
    let node = tree.node(id);
    let depth = node.depth;
    let bc = Style::default().fg(brace_color(depth as usize));
    let (open, close) = match node.kind {
        NodeKind::Object => ("{", "}"),
        NodeKind::Array => ("[", "]"),
        _ => unreachable!(),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(open.to_string(), bc));

    // Reserve room for the closer + (optionally) `…` + (for arrays) ` (<len>)`
    let count = node.children.len();
    let count_suffix = if matches!(node.kind, NodeKind::Array) {
        format!(" ({})", count)
    } else {
        String::new()
    };
    // Reserve for: close + count_suffix + possible ", …" tail (3 cols).
    // Over-reserves by 2 when nothing truncates, which is acceptable — the
    // summary just gets slightly narrower, and the total line always fits.
    let reserved = close.width() + count_suffix.width() + 3;
    let budget = max_width.saturating_sub(1 + reserved); // -1 for open already pushed

    let mut used = 0usize;
    let mut emitted = 0usize;

    for (i, &cid) in node.children.iter().enumerate() {
        let preview = preview_child(tree, cid);
        let chunk_w: usize = preview.iter().map(|s| s.content.as_ref().width()).sum();
        let sep_w = if i == 0 { 0 } else { 2 }; // ", "
        if used + sep_w + chunk_w > budget {
            break;
        }
        if i > 0 {
            spans.push(Span::styled(", ", Style::default().fg(COMMA_COLOR)));
            used += 2;
        }
        spans.extend(preview);
        used += chunk_w;
        emitted += 1;
    }

    if emitted < count {
        if emitted > 0 {
            spans.push(Span::styled(", ", Style::default().fg(COMMA_COLOR)));
        }
        spans.push(Span::styled("…".to_string(), Style::default().fg(OVERLAY0)));
    }
    spans.push(Span::styled(close.to_string(), bc));
    if !count_suffix.is_empty() {
        spans.push(Span::styled(count_suffix, Style::default().fg(OVERLAY0)));
    }
    spans
}

/// One-entry preview inside a summary. Objects show `key: value`;
/// arrays show just `value`. Nested containers render as `{…}` / `[…]`.
fn preview_child(tree: &Tree, cid: u32) -> Vec<Span<'static>> {
    let child = tree.node(cid);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(ref k) = child.key {
        spans.push(Span::styled(
            format!("\"{}\"", k),
            Style::default().fg(key_color(child.depth as usize)),
        ));
        spans.push(Span::styled(": ", Style::default().fg(COMMA_COLOR)));
    }
    match &child.kind {
        NodeKind::Null => spans.push(Span::styled(
            "null",
            Style::default()
                .fg(NULL_COLOR)
                .add_modifier(Modifier::ITALIC),
        )),
        NodeKind::Bool(b) => spans.push(Span::styled(
            if *b { "true" } else { "false" },
            Style::default().fg(BOOL_COLOR),
        )),
        NodeKind::Number(s) => spans.push(Span::styled(s.clone(), Style::default().fg(NUM_COLOR))),
        NodeKind::String(s) => {
            // In previews, clip long strings by column width (CJK-aware).
            // Budget = 20 display columns, matching "short summary" intent
            // without letting wide-character strings hog the line.
            const PREVIEW_BUDGET: usize = 20;
            let full_w = s.as_str().width();
            let text = if full_w <= PREVIEW_BUDGET {
                format!("\"{}\"", s)
            } else {
                let mut w = 0usize;
                let mut cut = 0usize;
                for (i, ch) in s.char_indices() {
                    let cw = ch.to_string().as_str().width();
                    if w + cw > PREVIEW_BUDGET {
                        break;
                    }
                    w += cw;
                    cut = i + ch.len_utf8();
                }
                format!("\"{}…\"", &s[..cut])
            };
            spans.push(Span::styled(text, Style::default().fg(STR_COLOR)));
        }
        NodeKind::Object => spans.push(Span::styled("{…}", Style::default().fg(FOLD_COLOR))),
        NodeKind::Array => spans.push(Span::styled("[…]", Style::default().fg(FOLD_COLOR))),
    }
    spans
}
